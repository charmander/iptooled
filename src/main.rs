#![feature(async_closure)]
#![feature(process_exitcode_placeholder)]
#![feature(try_blocks)]

mod tree;

use std::convert::{From, TryInto};
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use tokio::io::{self, AsyncReadExt};
use tokio::net::UnixListener;
use tokio::runtime;
use tokio::task;

use self::tree::{Address, AddressTree};

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
	let tree = Arc::new(AddressTree::new());
	let mut listener = UnixListener::bind(Path::new(socket_path))?;

	loop {
		let mut tree = tree.clone();
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

		task::spawn(async move {
			let request =
				match read_request(client).await {
					Ok(request) => request,
					Err(err) => {
						eprintln!("request read failed: {}", err);
						return;  // TODO: dropping the socket seems to close it, but is that reliable?
					},
				};

			match request {
				Request::Query(address) => {
					tree.query(address);
				}
				Request::Trust(address) => {
					Arc::get_mut(&mut tree).unwrap().record_trusted(address);
				}
				Request::Spam(address) => {
					Arc::get_mut(&mut tree).unwrap().record_spam(address);
				}
			}
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
