#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Header(u64);

const ID_MASK: u64 = 0x00000000ffffffff;
const FLAGS_MASK: u64 = 0xffffffff00000000;

#[repr(u64)]
pub enum Flags {
    // The occupied flag means this block actually contains data
    // and not garbage or unallocated. After some time of operation
    // normally all blocks get flag occupied set forever.
    // it's normally used first to know which blocks are free to use
    // until the full map is allocated
    Occupied = 0b0000_0001 << 32,
    // The dirty flag on the other hand is used to mark blocks as `modified`
    // from original form. And usually used later by the evict mechanism to see
    // if the evicted block should be committed to remote storage or not
    Dirty = 0b0000_0010 << 32,
}

impl From<u64> for Header {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Header {
    pub fn new(block: u32) -> Self {
        Self(block as u64)
    }
    pub fn block(&self) -> u32 {
        (self.0 & ID_MASK) as u32
    }

    pub fn flag(&self, flag: Flags) -> bool {
        self.0 & flag as u64 > 0
    }

    // pub fn with_block(self, id: u32) -> Self {
    //     ((self.0 & FLAGS_MASK) | id as u64).into()
    // }

    pub fn with_flag(self, flag: Flags, on: bool) -> Self {
        match on {
            true => self.0 | flag as u64,
            false => self.0 & !(flag as u64),
        }
        .into()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn id() {
        let header = Header::new(20);
        assert_eq!(20, header.block());
    }

    #[test]
    fn flags() {
        let header = Header::default();
        assert_eq!(false, header.flag(Flags::Dirty));

        let header = Header::new(20).with_flag(Flags::Dirty, true);
        assert_eq!(true, header.flag(Flags::Dirty));
        assert_eq!(20, header.block());

        let header = header.with_flag(Flags::Occupied, true);
        assert_eq!(true, header.flag(Flags::Dirty));
        assert_eq!(true, header.flag(Flags::Occupied));
        assert_eq!(20, header.block());
    }
}
