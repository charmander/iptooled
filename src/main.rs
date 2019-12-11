#![feature(async_closure)]
#![feature(process_exitcode_placeholder)]
#![feature(try_blocks)]

mod address;
mod tree;

use std::cell::RefCell;
use std::convert::{From, TryInto};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;
use tokio::fs::OpenOptions;
use tokio::io::{self, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, ErrorKind};
use tokio::net::{UnixListener, UnixStream};
use tokio::runtime;
use tokio::sync::mpsc;
use tokio::task;

use self::address::{ADDRESS_BYTES, Address};
use self::tree::{AddressTree, SerializedTreeOperation, TreeOperation};

#[derive(Clone, Debug)]
struct UsageError(&'static str);

impl Error for UsageError {}

impl fmt::Display for UsageError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

fn show_usage() {
	eprintln!("Usage: iptooled <persist-path> <socket-path>");
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
enum RequestType {
	Query,
	Trust,
	Spam,
}

impl RequestType {
	fn from(code: u8) -> Option<Self> {
		Some(
			match code {
				0 => Self::Query,
				1 => Self::Trust,
				2 => Self::Spam,
				_ => return None,
			}
		)
	}
}

#[derive(Clone, Debug)]
enum Request {
	Query(Address),
	Trust(Address),
	Spam(Address),
}

#[derive(Debug)]
enum ReadError {
	End,
	FormatError(Vec<u8>),
	IoError(io::Error),
}

impl Error for ReadError {}

impl fmt::Display for ReadError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{:?}", self)
	}
}

impl From<io::Error> for ReadError {
	fn from(error: io::Error) -> Self {
		Self::IoError(error)
	}
}

#[derive(Clone, Debug)]
struct ChecksumError {
	stored: u64,
	calculated: u64,
}

impl fmt::Display for ChecksumError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "stored checksum {} didn’t match calculated checksum {}", self.stored, self.calculated)
	}
}

impl Error for ChecksumError {}

async fn read_request(mut source: impl io::AsyncRead + Unpin) -> Result<Request, ReadError> {
	let mut buf = [0; 1 + ADDRESS_BYTES];
	let r = source.read(&mut buf).await?;

	if r == 0 {
		return Err(ReadError::End);
	}

	let request_type = RequestType::from(buf[0])
		.ok_or_else(|| ReadError::FormatError(buf[..r].to_vec()))?;

	if r < buf.len() {
		source.read_exact(&mut buf[r..]).await?;
	}

	let address = Address(buf[1..].try_into().unwrap());

	Ok(
		match request_type {
			RequestType::Query => Request::Query(address),
			RequestType::Trust => Request::Trust(address),
			RequestType::Spam => Request::Spam(address),
		}
	)
}

/// Persists writes to a file in a way that’s intended to not be able to accidentally produce a tree state that never existed. (For now, just by adding a checksum to every write, but there are more efficient options.)
async fn persist(mut log: impl AsyncWrite + Unpin, mut writes: mpsc::Receiver<SerializedTreeOperation>) -> io::Result<()> {
	while let Some(write) = writes.recv().await {
		log.write(&write.bytes).await?;
	}

	Ok(())
}

async fn interact(tree: Rc<RefCell<AddressTree>>, mut writer: mpsc::Sender<SerializedTreeOperation>, mut client: UnixStream) {
	let result: Result<!, ReadError> = try {
		loop {
			match read_request(&mut client).await? {
				Request::Query(address) => {
					let query_result = tree.borrow().query(&address);

					client.write(&[
						&query_result.trusted_count.to_be_bytes()[..],
						&query_result.spam_count.to_be_bytes()[..],
						&[query_result.prefix_bits][..],
					].concat()).await?;
				}
				Request::Trust(address) => {
					let write = tree.borrow_mut().record_trusted(address);
					writer.try_send(write).unwrap();
					client.write(&[0]).await?;
				}
				Request::Spam(address) => {
					let write = tree.borrow_mut().record_spam(address);
					writer.try_send(write).unwrap();
					client.write(&[0]).await?;
				}
			}
		}
	};

	match result {
		Ok(_) => unreachable!(),
		Err(ReadError::End) => {},
		Err(err) => eprintln!("client error: {}", err),
	}

	// TODO: dropping the socket seems to close it, but is that reliable?
}

/// Generates random keys for SipHash.
fn get_random_keys() -> Result<(u64, u64), getrandom::Error> {
	let mut buf = [0; 16];
	getrandom::getrandom(&mut buf)?;

	let key0 = u64::from_ne_bytes(buf[0..8].try_into().unwrap());
	let key1 = u64::from_ne_bytes(buf[8..16].try_into().unwrap());

	Ok((key0, key1))
}

async fn async_main(persist_path: OsString, socket_path: OsString) -> Result<(), Box<dyn Error>> {
	let (persist_log, initial_tree) =
		match OpenOptions::new().write(true).create_new(true).open(&persist_path).await {
			Ok(mut new_file) => {
				eprintln!("Writing checksum key to new file {:?}", persist_path);
				let keys = get_random_keys()?;
				new_file.write(&keys.0.to_be_bytes()).await?;
				new_file.write(&keys.1.to_be_bytes()).await?;
				(new_file, AddressTree::new_with_keys(keys.0, keys.1))
			},
			Err(err) if err.kind() == ErrorKind::AlreadyExists => {
				eprintln!("Restoring tree from {:?}", persist_path);

				// TODO: worth locking?
				let existing_file =
					OpenOptions::new()
						.read(true)
						.write(true)
						.open(&persist_path)
						.await?;

				let mut reader = BufReader::new(existing_file);
				let mut key_bytes = [0; 16];
				reader.read_exact(&mut key_bytes).await?;
				let key0 = u64::from_be_bytes(key_bytes[0..8].try_into().unwrap());
				let key1 = u64::from_be_bytes(key_bytes[8..16].try_into().unwrap());

				let mut initial_tree = AddressTree::new_with_keys(key0, key1);
				let mut restored_count = 0;

				loop {
					let mut operation_bytes = [0; 1 + ADDRESS_BYTES + 8];
					let r = reader.read(&mut operation_bytes).await?;

					if r == 0 {
						break;
					}

					if r < operation_bytes.len() {
						reader.read_exact(&mut operation_bytes[r..]).await?;
					}

					let checksum = u64::from_be_bytes(operation_bytes[1 + ADDRESS_BYTES..].try_into().unwrap());

					let operation = TreeOperation::deserialize(operation_bytes[..1 + ADDRESS_BYTES].try_into().unwrap());

					let applied = operation.apply(&mut initial_tree);

					let applied_checksum = u64::from_be_bytes(applied.bytes[1 + ADDRESS_BYTES..].try_into().unwrap());

					if checksum != applied_checksum {
						Err(ChecksumError {
							stored: checksum,
							calculated: applied_checksum,
						})?
					}

					restored_count += 1;
				}

				eprintln!("Restored {} operations", restored_count);

				let existing_file = reader.into_inner();

				(existing_file, initial_tree)
			},
			Err(err) => Err(err)?,
		};

	let tree = Rc::new(RefCell::new(initial_tree));
	let mut listener = UnixListener::bind(Path::new(&socket_path))?;
	let (writer, writes) = mpsc::channel(32);

	task::spawn_local(persist(persist_log, writes));

	loop {
		let client =
			match listener.accept().await {
				Err(err) => {
					eprintln!("accept failed: {}", err);
					continue;
				}
				Ok((client, _)) => {
					eprintln!("new client: {:?}", client.peer_cred());
					client
				}
			};

		task::spawn_local(interact(tree.clone(), writer.clone(), client));
	}
}

fn main() -> ExitCode {
	let result: Result<(), Box<dyn Error>> = try {
		let mut args = env::args_os();
		let _ = args.next();

		let persist_path =
			match args.next() {
				Some(path) => path,
				None => {
					show_usage();
					Err(UsageError("Persist path is required"))?
				},
			};

		let socket_path =
			match args.next() {
				Some(path) => path,
				None => {
					show_usage();
					Err(UsageError("Socket path is required"))?
				},
			};

		let mut single_threaded_runtime =
			runtime::Builder::new()
				.enable_io()
				.basic_scheduler()
				.build()?;

		let local = task::LocalSet::new();

		local.block_on(
			&mut single_threaded_runtime,
			async_main(persist_path, socket_path)
		)?
	};

	match result {
		Ok(()) => ExitCode::SUCCESS,
		Err(err) => {
			eprintln!("Error: {}", err);
			ExitCode::FAILURE
		},
	}
}
