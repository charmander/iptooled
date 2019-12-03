#![feature(async_closure)]
#![feature(process_exitcode_placeholder)]
#![feature(try_blocks)]

mod address;
mod tree;

use std::convert::{From, TryInto};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::runtime;
use tokio::task;

use self::address::{ADDRESS_BYTES, Address};
use self::tree::AddressTree;

#[derive(Clone, Debug)]
struct UsageError(&'static str);

impl Error for UsageError {}

impl fmt::Display for UsageError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

fn show_usage() {
	eprintln!("Usage: iptooled <socket-path>");
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

async fn async_main(socket_path: OsString) -> Result<(), Box<dyn Error>> {
	let tree = Rc::new(AddressTree::new());
	let mut listener = UnixListener::bind(Path::new(&socket_path))?;

	loop {
		let mut tree = tree.clone();
		let mut client =
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

		task::spawn_local(async move {
			let result: Result<!, ReadError> = try {
				loop {
					match read_request(&mut client).await? {
						Request::Query(address) => {
							let query_result = tree.query(address);

							client.write(&[
								&query_result.trusted_count.to_be_bytes()[..],
								&query_result.spam_count.to_be_bytes()[..],
								&[query_result.prefix_bits][..],
							].concat()).await?;
						}
						Request::Trust(address) => {
							Rc::get_mut(&mut tree).unwrap().record_trusted(address);
							client.write(&[0]).await?;
						}
						Request::Spam(address) => {
							Rc::get_mut(&mut tree).unwrap().record_spam(address);
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
		});
	}
}

fn main() -> ExitCode {
	let result: Result<(), Box<dyn Error>> = try {
		let socket_path =
			match env::args_os().nth(1) {
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
			async_main(socket_path)
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
