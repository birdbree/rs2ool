//! Floors: one floor material per `flo.dat` entry, their names in a string
//! table, and the `flo id → material id` map in the materials manifest.

use std::collections::BTreeMap;

use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2pack::records::material::{Material, flags, mode};
use rs2pack::records::strings;
use rs2pack::{Kind, PackWriter};

use crate::palette::Palette;
use crate::{ConvertError, material_id};

/// String-table domain for floors (records `domain << 8 | locale`).
pub const STRINGS_DOMAIN: u32 = 1;

/// The floors' materials pack and strings pack (locale 0).
pub fn packs(
    records: &Records,
    palette: &Palette,
    source_cache_crc: u32,
) -> Result<(PackWriter, PackWriter), ConvertError> {
    let mut materials = PackWriter::new(Kind::Materials, source_cache_crc);
    let mut names = Vec::new();
    let mut table = BTreeMap::new();
    for id in records.ids(RecordType::Flo)? {
        let Record::Flo(f) = records.decode(RecordType::Flo, id)? else {
            unreachable!()
        };
        let mid = material_id::FLOOR_BASE + id as u16;
        let mut fl = flags::FLOOR;
        if f.hide_underlay {
            fl |= flags::HIDE_UNDERLAY;
        }
        if f.occlude {
            fl |= flags::OCCLUDE;
        }
        let m = match f.texture {
            Some(t) => Material {
                id: mid,
                mode: mode::TEXTURE,
                texture: t.into(),
                period: 1,
                palette: 0,
                roughness: 255,
                flags: fl,
                alpha: 255,
            },
            None => Material {
                id: mid,
                mode: mode::PALETTE,
                texture: 0,
                period: 0,
                palette: palette.index_rgb(f.rgb.unwrap_or(0)),
                roughness: 255,
                flags: fl,
                alpha: 255,
            },
        };
        materials.insert(mid.into(), m.encode());
        let name = names.len();
        names.push(f.name.unwrap_or_default());
        table.insert(
            id.to_string(),
            serde_json::json!({ "material": mid, "name": name }),
        );
    }
    materials.set_extra(
        "floors",
        serde_json::to_value(table).expect("table serialises"),
    );
    let mut strings_pack = PackWriter::new(Kind::Strings, source_cache_crc);
    strings_pack.insert(STRINGS_DOMAIN << 8, strings::encode(&names));
    Ok((materials, strings_pack))
}
