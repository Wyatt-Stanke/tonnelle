use rand::RngExt;
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

    pub fn parse(cidr: &str) -> Result<Self, String> {
        let mut parts = cidr.split('/');
        let ip_str = parts.next().ok_or("Missing IP part")?;
        let prefix_len_str = parts.next().ok_or("Missing prefix length part")?;
        let prefix_len = prefix_len_str
            .parse::<u32>()
            .map_err(|_| "Invalid prefix length")?;
        if prefix_len > 128 {
            return Err(format!(
                "Prefix length {} is out of range (must be 0..=128)",
                prefix_len
            ));
        }
        let base_ip = std::net::Ipv6Addr::from_str(ip_str).map_err(|_| "Invalid IPv6 address")?;
        Ok(Ipv6Cidr::new(base_ip, prefix_len))
    }

    pub fn generate_random_ipv6_in_subnet(&self) -> std::net::Ipv6Addr {
        let host_bits = 128 - self.prefix_len;
        // Use checked_shl to avoid overflow when host_bits == 128 (prefix_len == 0).
        let host_mask = 1u128
            .checked_shl(host_bits)
            .map_or(u128::MAX, |v| v - 1);
        let masked = u128::from(self.base_ip) & !host_mask;
        let random_bits = rand::rng().random::<u128>() & host_mask;
        std::net::Ipv6Addr::from(masked | random_bits)
    }
}
