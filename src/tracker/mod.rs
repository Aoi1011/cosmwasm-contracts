use std::net::{SocketAddr, ToSocketAddrs};

use anyhow::{anyhow, Context};

pub mod http;
pub mod udp;

pub struct Tracker {
}

pub enum Addr {
    Udp(SocketAddr),
    Http(SocketAddr),
}

pub fn get_addr(announce: &str) -> anyhow::Result<Addr> {
    if let Some((protocol, addr)) = announce.split_once("://") {
        match protocol {
            "http" => Ok(Addr::Http(
                announce
                    .to_socket_addrs()
                    .context("parse socket addr")?
                    .next()
                    .unwrap(),
            )),
            "udp" => {
                if let Some((url, _)) = addr.split_once("/announce") {
                    Ok(Addr::Udp(
                        url.to_socket_addrs()
                            .context("parse socket addr")?
                            .next()
                            .unwrap(),
                    ))
                } else {
                    Err(anyhow!("cannot find announce"))
                }
            }
            protocol => Err(anyhow!("does not support: {protocol}")),
        }
    } else {
        Err(anyhow!("cannot find announce"))
    }
}
