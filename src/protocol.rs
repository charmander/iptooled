use std::error::Error;
use std::fmt;
use tokio::io::{self, AsyncRead, AsyncReadExt, BufReader, ErrorKind};

use super::address::{ADDRESS_BYTES, Address};
use super::tree::{USER_BYTES, User};

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
pub enum Request {
	Query(Address),
	Trust(Address, User),
	Spam(Address, User),
}

#[derive(Debug)]
pub enum ReadError {
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

pub async fn read_request<T: AsyncRead + Unpin>(source: &mut BufReader<T>) -> Result<Request, ReadError> {
	let request_type_byte =
		match source.read_u8().await {
			Err(err) if err.kind() == ErrorKind::UnexpectedEof => return Err(ReadError::End),
			other => other?,
		};

	let request_type =
		match RequestType::from(request_type_byte) {
			Some(t) => t,
			None => {
				let mut context = [0; 1 + ADDRESS_BYTES + USER_BYTES];
				context[0] = request_type_byte;
				let r = source.read(&mut context[1..]).await?;
				return Err(ReadError::FormatError(context[..1 + r].to_vec()));
			},
		};

	let mut address = [0; ADDRESS_BYTES];
	source.read_exact(&mut address).await?;
	let address = Address(address);

	let get_user = async move || -> io::Result<User> {
		let mut user = [0; USER_BYTES];
		source.read_exact(&mut user).await?;
		Ok(User::from_bytes(user))
	};

	Ok(
		match request_type {
			RequestType::Query => Request::Query(address),
			RequestType::Trust => Request::Trust(address, get_user().await?),
			RequestType::Spam => Request::Spam(address, get_user().await?),
		}
	)
}
