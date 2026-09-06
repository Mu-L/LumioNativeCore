//! FNV-1a 64 over the canonical definition encoding (contract §8). Hand-written so the
//! value is stable across Rust releases (`DefaultHasher` is not).

pub(crate) const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
pub(crate) const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
pub(crate) const NONE: u32 = u32::MAX;

pub(crate) struct Fnv1a(u64);

impl Fnv1a {
    pub(crate) const fn new() -> Self {
        Self(FNV_OFFSET)
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    pub(crate) fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub(crate) fn opt(&mut self, v: Option<u32>) {
        self.u32(v.unwrap_or(NONE));
    }

    pub(crate) fn list(&mut self, items: impl ExactSizeIterator<Item = u32>) {
        self.u32(items.len() as u32);
        for v in items {
            self.u32(v);
        }
    }

    pub(crate) const fn finish(&self) -> u64 {
        self.0
    }
}
