#![feature(async_closure)]
#![feature(const_int_conversion)]
#![feature(process_exitcode_placeholder)]
#![feature(try_blocks)]

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

mod address;
mod protocol;
mod time_list;
mod tree;

use std::cell::RefCell;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::runtime;
use tokio::task;

use self::protocol::{ReadError, Request, read_request};
use self::time_list::CoarseSystemTime;
use self::tree::SpamTree;

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

async fn interact(tree: Rc<RefCell<SpamTree>>, mut client: UnixStream) {
	let (client_read, mut client_write) = client.split();
	let mut reader = BufReader::new(client_read);

	let result: Result<!, ReadError> = try {
		loop {
			match read_request(&mut reader).await? {
				Request::Query(address) => {
					let query_result = tree.borrow_mut().query(&address, CoarseSystemTime::now());
					let mut response = [0; 9];

					response[0..4].copy_from_slice(&query_result.stats.trusted_users.to_be_bytes());
					response[4..8].copy_from_slice(&query_result.stats.spam_users.to_be_bytes());
					response[8] = query_result.prefix_bits;

					client_write.write_all(&response).await?;
				}
				Request::Trust(address, user) => {
					tree.borrow_mut().trust(address, user, CoarseSystemTime::now());
					client_write.write_u8(0).await?;
				}
				Request::Spam(address, user) => {
					tree.borrow_mut().spam(address, user, CoarseSystemTime::now());
					client_write.write_u8(0).await?;
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

async fn async_main(socket_path: OsString) -> Result<(), Box<dyn Error>> {
	let tree = Rc::new(RefCell::new(SpamTree::new()));
	let mut listener = UnixListener::bind(Path::new(&socket_path))?;

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

		task::spawn_local(interact(tree.clone(), client));
	}
}

fn main() -> ExitCode {
	let result: Result<(), Box<dyn Error>> = try {
		let mut args = env::args_os();
		let _ = args.next();

		let socket_path =
			match args.next() {
				Some(path) => path,
				None => {
					show_usage();
					Err(UsageError("Socket path is required"))?
				},
			};

		if !args.next().is_none() {
			show_usage();
			Err(UsageError("Too many arguments"))?;
		}

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
