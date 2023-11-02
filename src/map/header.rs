#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Header(u64);

const ID_MASK: u64 = 0x00000000ffffffff;
const FLAGS_MASK: u64 = 0xffffffff00000000;

#[repr(u64)]
pub enum Flags {
    Occupied = 0b0000_0001 << 32,
    Dirty = 0b0000_0010 << 32,
}

impl From<u64> for Header {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Header {
    pub fn id(&self) -> u32 {
        (self.0 & ID_MASK) as u32
    }

    pub fn flag(&self, flag: Flags) -> bool {
        self.0 & flag as u64 > 0
    }

    pub fn with_id(self, id: u32) -> Self {
        ((self.0 & FLAGS_MASK) | id as u64).into()
    }

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
        let header = Header::default().with_id(20);
        assert_eq!(20, header.id());
    }

    #[test]
    fn flags() {
        let header = Header::default();
        assert_eq!(false, header.flag(Flags::Dirty));

        let header = header.with_id(20).with_flag(Flags::Dirty, true);
        assert_eq!(true, header.flag(Flags::Dirty));
        assert_eq!(20, header.id());

        let header = header.with_flag(Flags::Occupied, true);
        assert_eq!(true, header.flag(Flags::Dirty));
        assert_eq!(true, header.flag(Flags::Occupied));
        assert_eq!(20, header.id());
    }
}
