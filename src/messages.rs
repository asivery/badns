pub const RR_A: u16 = 1;
pub const RR_SOA: u16 = 6;
pub const RR_PTR: u16 = 12;
pub const RR_CNAME: u16 = 5;
pub const RR_TXT: u16 = 16;
pub const RR_AAAA: u16 = 28;
pub const RR_SRV: u16 = 33;

pub const SUPPORTED_RR: [u16; 7] = [RR_A, RR_CNAME, RR_SOA, RR_PTR, RR_TXT, RR_AAAA, RR_SRV];
pub const SUPPORTED_RR_NAMES: [&'static str; 7] = [
    "RR_A", "RR_CNAME", "RR_SOA", "RR_PTR", "RR_TXT", "RR_AAAA", "RR_SRV",
];
