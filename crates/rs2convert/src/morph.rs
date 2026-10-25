//! Morph tables: which child a location or NPC shows for a variable's
//! value, as the original client picks it.
//!
//! ```text
//! varbit   value = (varp[varbit.varp] >> lsb) & mask(msb - lsb)
//! varp     value = varp[varp]
//! mask(n)  = 2^(n + 1) - 1, wrapping like a 32-bit int (so mask(31) = -1)
//! child    = children[value], or nothing when value < 0, value ≥ len, or the slot is empty
//! ```
//!
//! A definition with a morph table never draws itself: the client draws the
//! child the value picks, or nothing.

use rs2cache::Records;
use rs2cache::types::config::Morph;
use rs2cache::types::record::{Record, RecordType};

/// The variable a morph table reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variable {
    /// Bits `lsb..=msb` of varp `varp`, as varbit `id` defines them.
    Varbit {
        /// The varbit.
        id: u16,
        /// The varp it lives in.
        varp: u16,
        /// Its lowest bit.
        lsb: u8,
        /// Its highest bit.
        msb: u8,
    },
    /// The whole of a varp.
    Varp(u16),
}

/// The original client's bit mask for a varbit `width` bits past its
/// lowest: `2^(width + 1) - 1`, wrapping like a 32-bit int.
pub fn mask(width: u8) -> i32 {
    ((2i64 << (width & 31)) - 1) as i32
}

impl Variable {
    /// The variable morph table `m` reads: its varbit (decoded for its bit
    /// range) if it names one, else its varp. `None` when it names neither,
    /// or its varbit won't decode.
    pub fn of(records: &Records, m: &Morph) -> Option<Self> {
        if let Some(id) = m.varbit {
            return match records.decode(RecordType::Varbit, id.into()) {
                Ok(Record::Varbit(d)) => Some(Variable::Varbit {
                    id,
                    varp: d.varp,
                    lsb: d.lsb,
                    msb: d.msb,
                }),
                _ => None,
            };
        }
        m.varp.map(Variable::Varp)
    }

    /// The varp the variable lives in.
    pub fn varp(&self) -> u16 {
        match *self {
            Variable::Varbit { varp, .. } | Variable::Varp(varp) => varp,
        }
    }

    /// The value it has when its varp holds `contents`.
    pub fn read(&self, contents: i32) -> i32 {
        match *self {
            Variable::Varbit { lsb, msb, .. } => {
                (contents >> (lsb & 31)) & mask(msb.wrapping_sub(lsb))
            }
            Variable::Varp(_) => contents,
        }
    }

    /// Varp contents `contents` with the variable set to `value`; the
    /// varp's other bits are kept.
    pub fn write(&self, contents: i32, value: i32) -> i32 {
        match *self {
            Variable::Varbit { lsb, msb, .. } => {
                let m = mask(msb.wrapping_sub(lsb)).wrapping_shl(u32::from(lsb & 31));
                (contents & !m) | (value.wrapping_shl(u32::from(lsb & 31)) & m)
            }
            Variable::Varp(_) => value,
        }
    }

    /// The largest value it can hold.
    pub fn max(&self) -> i32 {
        match *self {
            Variable::Varbit { lsb, msb, .. } => mask(msb.wrapping_sub(lsb)).max(0),
            Variable::Varp(_) => i32::MAX,
        }
    }
}

/// The slot of `m`'s children that `value` picks: `None` when it's out of
/// range or the slot is empty, so the client draws nothing.
pub fn pick(m: &Morph, value: i32) -> Option<usize> {
    let i = usize::try_from(value).ok()?;
    m.children.get(i)?.map(|_| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varbit(lsb: u8, msb: u8) -> Variable {
        Variable::Varbit {
            id: 1,
            varp: 2,
            lsb,
            msb,
        }
    }

    #[test]
    fn masks_match_the_client_table() {
        assert_eq!(mask(0), 1);
        assert_eq!(mask(2), 7);
        assert_eq!(mask(30), i32::MAX);
        assert_eq!(mask(31), -1);
    }

    #[test]
    fn varbits_read_and_write_their_bits() {
        let v = varbit(4, 6);
        assert_eq!(v.max(), 7);
        // Bits 4–6 of 0b1101_0011 are 0b101.
        assert_eq!(v.read(0b1101_0011), 0b101);
        // Writing keeps the other bits and drops value bits that don't fit.
        assert_eq!(v.write(0b1101_0011, 0b010), 0b1010_0011);
        assert_eq!(v.read(v.write(-1, 3)), 3);
        assert_eq!(v.write(0, 0xff), 0b111_0000);
        // A whole varp reads and writes as itself.
        let p = Variable::Varp(9);
        assert_eq!((p.read(-5), p.write(3, 12), p.varp()), (-5, 12, 9));
        // A negative varp still shifts arithmetically, as a Java int does.
        assert_eq!(varbit(28, 31).read(i32::MIN), 0b1000);
    }

    #[test]
    fn values_pick_children_or_nothing() {
        let m = Morph {
            varbit: None,
            varp: Some(3),
            children: vec![Some(10), None, Some(12)],
        };
        assert_eq!(pick(&m, 0), Some(0));
        assert_eq!(pick(&m, 1), None, "an empty slot draws nothing");
        assert_eq!(pick(&m, 2), Some(2));
        assert_eq!(pick(&m, 3), None, "past the end");
        assert_eq!(pick(&m, -1), None);
    }
}
