use std::{net::{Ipv4Addr, SocketAddr, SocketAddrV4, IpAddr, Ipv6Addr}, collections::{HashMap, hash_map::Entry}, str::FromStr};

use pnet::datalink::NetworkInterface;

// TODO handle SIGINT