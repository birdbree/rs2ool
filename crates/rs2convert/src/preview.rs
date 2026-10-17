//! The model a record describes, assembled as the client draws it, for
//! viewers: NPCs merged, recoloured and scaled; objects shaped for type 10
//! (or their first type); items as their inventory model; identity-kit
//! parts merged; spot animations recoloured and scaled.

use rs2cache::Records;
use rs2intermediate::config::Recolour;
use rs2intermediate::model::Model;
use rs2intermediate::record::{Record, RecordType};

use crate::ConvertError;
use crate::creature::{merge, recolour};
use crate::deform::apply_scale;

fn load(records: &Records, id: u16) -> Result<Model, ConvertError> {
    match records.decode(RecordType::Model, id.into())? {
        Record::Model(m) => Ok(m),
        _ => unreachable!(),
    }
}

fn assemble(
    records: &Records,
    ids: &[u16],
    recolours: &[Recolour],
    scale: [i32; 3],
) -> Result<Option<Model>, ConvertError> {
    if ids.is_empty() {
        return Ok(None);
    }
    let parts = ids
        .iter()
        .map(|&m| load(records, m))
        .collect::<Result<Vec<_>, _>>()?;
    let mut m = merge(&parts);
    recolour(&mut m, recolours);
    apply_scale(&mut m.vertices, scale);
    Ok(Some(m))
}

/// The model `record` describes, if it describes one.
pub fn model_for(records: &Records, record: &Record) -> Result<Option<Model>, ConvertError> {
    let s = |v: u16| i32::from(v);
    match record {
        Record::Model(m) => Ok(Some(m.clone())),
        Record::Npc(d) => assemble(
            records,
            &d.models,
            &d.recolours,
            [s(d.scale_xz), s(d.scale_y), s(d.scale_xz)],
        ),
        Record::Obj(d) => assemble(
            records,
            &[d.inventory_model],
            &d.recolours,
            [s(d.scale_x), s(d.scale_y), s(d.scale_z)],
        ),
        Record::Idk(d) => assemble(records, &d.models, &d.recolours, [128; 3]),
        Record::SpotAnim(d) => match d.model {
            Some(m) => assemble(
                records,
                &[m],
                &d.recolours,
                [s(d.scale_xy), s(d.scale_z), s(d.scale_xy)],
            ),
            None => Ok(None),
        },
        Record::Loc(d) => {
            let models = crate::object::models_of(records, d)?;
            let pick = models.iter().position(|(k, _)| *k == 10).unwrap_or(0);
            Ok(models.into_iter().nth(pick).map(|(_, mut m)| {
                recolour(&mut m, &d.recolours);
                m
            }))
        }
        _ => Ok(None),
    }
}
