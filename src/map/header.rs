#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Header(u64);

const ID_MASK: u64 = 0x00000000ffffffff;

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

impl Header {
    /// create a new header with block index
    pub fn new(block: u32) -> Self {
        Self(block as u64)
    }
    /// gets the block index
    pub fn block(&self) -> u32 {
        (self.0 & ID_MASK) as u32
    }

    pub fn set_block(&mut self, id: u32) -> &mut Self {
        self.0 |= id as u64 & ID_MASK;
        self
    }

    /// gets if a flag is set on a header
    pub fn flag(&self, flag: Flags) -> bool {
        self.0 & flag as u64 > 0
    }

    /// sets or unsets a flag on a header
    pub fn set(&mut self, flag: Flags, on: bool) -> &mut Self {
        let v = match on {
            true => self.0 | flag as u64,
            false => self.0 & !(flag as u64),
        };

        self.0 = v;
        self
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

        let mut header = Header::new(20);
        header.set(Flags::Dirty, true);
        assert_eq!(true, header.flag(Flags::Dirty));
        assert_eq!(20, header.block());

        header.set_block(30);
        header.set(Flags::Occupied, true);
        assert_eq!(true, header.flag(Flags::Dirty));
        assert_eq!(true, header.flag(Flags::Occupied));
        assert_eq!(30, header.block());
    }
}
