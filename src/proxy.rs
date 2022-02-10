use std::{net::{UdpSocket, SocketAddr, SocketAddrV4, Ipv4Addr, Ipv6Addr, SocketAddrV6, IpAddr}, hash::{Hash, Hasher}, str::FromStr};

fn hex(bytes: &[u8]) -> String {
	let mut hex = String::new();
	for byte in bytes {
		hex.push_str(&format!("{:02x} ", byte));
	}
	hex.pop();
	hex
}

fn hash(bytes: &[u8]) -> u64 {
	let mut hasher = std::collections::hash_map::DefaultHasher::new();
	bytes.hash(&mut hasher);
	hasher.finish()
}

#[derive(Debug)]
#[repr(transparent)]
struct Socket(UdpSocket);
impl From<UdpSocket> for Socket {
	#[inline]
	fn from(socket: UdpSocket) -> Self {
		Self(socket)
	}
}
impl Socket {
	fn set_options(&self) -> Result<(), std::io::Error> {
		#[cfg(target_os = "linux")] unsafe {
			use std::os::unix::io::AsRawFd;

			macro_rules! setsockopt {
				($opt:ident) => {
					let optval: libc::c_int = 1;
					let ret = libc::setsockopt(
						self.0.as_raw_fd(),
						libc::SOL_SOCKET,
						libc::$opt,
						&optval as *const _ as *const libc::c_void,
						core::mem::size_of_val(&optval) as libc::socklen_t,
					);
					if ret != 0 {
						dbg!(stringify!($opt));
						return Err(dbg!(std::io::Error::last_os_error()));
					}
				}
			}

			setsockopt!(SO_KEEPALIVE);
			setsockopt!(SO_REUSEPORT);
			setsockopt!(SO_OOBINLINE);
		}
		Ok(())
	}
}
impl core::ops::Deref for Socket {
	type Target = UdpSocket;

	#[inline]
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

pub fn proxy_server(proxy_rx: SocketAddr, game_tx: SocketAddr) -> Result<(), std::io::Error> {
	let socket = Socket(UdpSocket::bind(proxy_rx)?);
	socket.set_options()?;

	// socket.set_read_timeout(Some(Duration::from_secs(10)))?;
	// socket.set_write_timeout(Some(Duration::from_secs(10)))?;

	println!("{:?}", socket);

	let game_server = {
		let game_server = Socket(UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0))?);
		game_server.set_options()?;
		game_server.connect(game_tx)?;
		game_server
	};

	println!("{:?} {:?} {:?} {:?}", socket, socket.peer_addr(), game_server, game_server.peer_addr());

	let _socket = socket.try_clone()?;
	let _game_server = game_server.try_clone()?;

	std::thread::spawn(move || {
		loop {
			let mut buf = [0u8; u16::MAX as usize + 6];

			let len = game_server.recv(&mut buf)?;
			let buf = &buf[0..len];

			let ip: [u8; 4] = unsafe { buf[0..4].try_into().unwrap_unchecked() };
			let port: [u8; 2] = unsafe { buf[4..6].try_into().unwrap_unchecked() };
			let player_addr = SocketAddrV4::new(Ipv4Addr::from(u32::from_be_bytes(ip)), u16::from_be_bytes(port));

			let buf = &buf[6..];

			println!("[game -> {player_addr}]\nlen: {}\nhash: {:x}\n{:?}\n{}\n", buf.len(), hash(buf), String::from_utf8_lossy(buf), hex(buf));

			socket.send_to(buf, player_addr);
		}

		Ok::<_, std::io::Error>(())
	});

	let socket = _socket;
	let game_server = _game_server;

	loop {
		let mut buf = [0u8; u16::MAX as usize + 4 + 2];
		let (len, addr) = socket.recv_from(&mut buf[6..])?;
		let buf = &mut buf[0..len + 6];

		let addr = match addr {
			SocketAddr::V4(ip) => ip,
			SocketAddr::V6(_) => {
				println!("wtf wanted a ipv4");
				continue
			}
		};

		println!("[{addr} -> game]\nlen: {}\nhash: {:x}\n{:?}\n{}\n", buf.len() - 6, hash(&buf[6..]), String::from_utf8_lossy(&buf[6..]), hex(&buf[6..]));

		buf[0..4].copy_from_slice(&u32::to_be_bytes((*addr.ip()).into()));
		buf[4..6].copy_from_slice(&addr.port().to_be_bytes());

		game_server.send(buf)?;
	}

	Ok(())
}

fn main() {
	proxy_server(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 27018), SocketAddr::new(IpAddr::V6(Ipv6Addr::from_str("2a01:4b00:961a:ab00:a62:66ff:fe49:c7c4").unwrap()), 17017)).unwrap();
}