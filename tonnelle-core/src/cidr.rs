use rand::Rng;
use std::str::FromStr;

pub struct Ipv6Cidr {
    base_ip: std::net::Ipv6Addr,
    prefix_len: u32,
}

impl Ipv6Cidr {
    pub fn new(base_ip: std::net::Ipv6Addr, prefix_len: u32) -> Self {
        Ipv6Cidr {
            base_ip,
            prefix_len,
        }
    }

    pub fn parse(cidr: &str) -> Self {
        let mut parts = cidr.split('/');
        let ip_str = parts.next().unwrap();
        let prefix_len = parts.next().unwrap().parse::<u32>().unwrap();
        let base_ip = std::net::Ipv6Addr::from_str(ip_str).unwrap();
        Ipv6Cidr::new(base_ip, prefix_len)
    }

    pub fn generate_random_ipv6_in_subnet(&self) -> std::net::Ipv6Addr {
        let masked = u128::from(self.base_ip) & (!((1u128 << (128 - self.prefix_len)) - 1));
        let random_bits =
            rand::thread_rng().gen::<u128>() & ((1u128 << (128 - self.prefix_len)) - 1);
        std::net::Ipv6Addr::from(masked | random_bits)
    }
}
