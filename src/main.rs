#![feature(async_closure)]
#![feature(process_exitcode_placeholder)]
#![feature(try_blocks)]

mod address;
mod tree;

use std::convert::{From, TryInto};
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::runtime;
use tokio::task;

use self::address::Address;
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

#[derive(Clone, Debug)]
enum Request {
	Query(Address),
	Trust(Address),
	Spam(Address),
}

#[derive(Debug)]
enum ReadError {
	FormatError([u8; 17]),
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
	let mut buf = [0; 17];
	source.read_exact(&mut buf).await?;

	let request_type = buf[0];
	let address = Address(buf[1..].try_into().unwrap());

	Ok(
		match request_type {
			0 => Request::Query(address),
			1 => Request::Trust(address),
			2 => Request::Spam(address),
			_ => Err(ReadError::FormatError(buf))?,
		}
	)
}

async fn async_main(socket_path: &OsStr) -> Result<(), Box<dyn Error>> {
	let tree = Rc::new(AddressTree::new());
	let mut listener = UnixListener::bind(Path::new(socket_path))?;

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
			let err: Result<!, ReadError> = try {
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

			eprintln!("client error: {}", err.err().unwrap());
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

		single_threaded_runtime.block_on(async_main(&socket_path))?
	};

	match result {
		Ok(()) => ExitCode::SUCCESS,
		Err(err) => {
			eprintln!("Error: {}", err);
			ExitCode::FAILURE
		},
	}
}
