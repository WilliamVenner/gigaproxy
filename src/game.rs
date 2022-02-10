use std::{net::{UdpSocket, SocketAddr, SocketAddrV4, Ipv4Addr, Ipv6Addr, SocketAddrV6}, collections::HashMap, thread::JoinHandle, hash::{Hash, Hasher}};

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
struct JoinOnDropHandle<T>(Option<JoinHandle<T>>);
impl<T> From<JoinHandle<T>> for JoinOnDropHandle<T> {
	fn from(handle: JoinHandle<T>) -> Self {
		JoinOnDropHandle(Some(handle))
	}
}
impl<T> Drop for JoinOnDropHandle<T> {
	fn drop(&mut self) {
		if let Some(handle) = self.0.take() {
			println!("joining thread...");
			let _ = handle.join();
		}
	}
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

	#[inline]
	fn try_clone(&self) -> Result<Self, std::io::Error> {
		self.0.try_clone().map(Into::into)
	}
}
impl core::ops::Deref for Socket {
	type Target = UdpSocket;

	#[inline]
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[derive(Debug)]
struct GameProxy {
	players: HashMap<SocketAddrV4, (JoinOnDropHandle<()>, Socket), twox_hash::RandomXxHashBuilder64>,
	proxy: Socket,
	game: SocketAddrV4
}
impl GameProxy {
	fn init(proxy: SocketAddr, game: SocketAddrV4) -> Result<(), std::io::Error> {
		let mut proxy = dbg!(GameProxy {
			proxy: Socket(UdpSocket::bind(proxy)?),
			players: HashMap::with_capacity_and_hasher(128, twox_hash::RandomXxHashBuilder64::default()),
			game
		});

		proxy.proxy.set_options()?;

		// proxy.proxy.set_read_timeout(Some(Duration::from_secs(10)))?;
		// proxy.proxy.set_write_timeout(Some(Duration::from_secs(10)))?;

		loop {
			if let Err(err) = proxy.poll_proxy_channel() {
				println!("{:#?}", err);
			}
		}
	}

	#[inline]
	fn poll_proxy_channel(&mut self) -> Result<(), std::io::Error> {
		let mut buf = [0u8; u16::MAX as usize + 6];
		let (len, proxy_addr) = self.proxy.recv_from(&mut buf)?;
		let buf = &buf[0..len];

		let ip: [u8; 4] = unsafe { buf[0..4].try_into().unwrap_unchecked() };
		let port: [u8; 2] = unsafe { buf[4..6].try_into().unwrap_unchecked() };
		let player_addr = SocketAddrV4::new(Ipv4Addr::from(u32::from_be_bytes(ip)), u16::from_be_bytes(port));

		let buf = &buf[6..];

		println!("[{player_addr} -> game]\nlen: {}\nhash: {:x}\n{:?}\n{}\n", buf.len(), hash(buf), String::from_utf8_lossy(&buf), hex(&buf));

		self.get_player(proxy_addr, player_addr)?.send(buf)?;
		Ok(())
	}

	#[inline]
	fn poll_player_channel(proxy_addr: SocketAddr, proxy: &Socket, player_addr: SocketAddrV4, player: &Socket) -> Result<(), std::io::Error> {
		//println!("poll_player_channel");

		let mut buf = [0u8; u16::MAX as usize + 6];
		let len = player.recv(&mut buf[6..])?;
		let buf = &mut buf[0..len + 6];

		buf[0..4].copy_from_slice(&u32::from(*player_addr.ip()).to_be_bytes());
		buf[4..6].copy_from_slice(&player_addr.port().to_be_bytes());

		println!("[game -> {player_addr}]\nlen: {}\nhash: {:x}\n{:?}\n{}\n", buf.len() - 6, hash(&buf[6..]), String::from_utf8_lossy(&buf[6..]), hex(&buf[6..]));

		proxy.send_to(&buf, proxy_addr)?;

		Ok(())
	}

	fn get_player(&mut self, proxy: SocketAddr, player: SocketAddrV4) -> Result<&Socket, std::io::Error> {
		Ok(match self.players.entry(player) {
			std::collections::hash_map::Entry::Occupied(o) => &o.into_mut().1,
			std::collections::hash_map::Entry::Vacant(v) => &v.insert({
				let socket = Socket(UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))?);
				socket.set_options()?;
				socket.connect(self.game)?;

				let socket_ref = socket.try_clone()?;
				let proxy_ref = self.proxy.try_clone()?;
				let player = player;

				(
					std::thread::spawn(move || {
						loop {
							if let Err(err) = Self::poll_player_channel(proxy, &proxy_ref, player, &socket_ref) {
								dbg!(err);
							}
						}
					}).into(),

					socket
				)
			}).1,
		})
	}
}

fn main() {
	GameProxy::init(
		SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 17017, 0, 0)),
		SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 96), 27018)
	).unwrap();
}