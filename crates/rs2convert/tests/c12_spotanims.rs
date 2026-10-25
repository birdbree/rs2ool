//! Every spot animation converts: a model at `SPOTANIM_MODEL | id`, and
//! with a sequence a rig at `SPOTANIM_RIG | id` and a clip whose keys
//! carry the scaling, listed in the models pack's `spotanims` table. Most
//! clips validate, scaling ones included, and converting twice gives the
//! same packs.

mod common;

use contentpack::records::anim::{Clip, SCALE_ONE};
use contentpack::{Kind, PackReader};
use rs2cache::Records;
use rs2cache::types::record::RecordType;
use rs2convert::pipeline::ids;
use rs2convert::run;

#[test]
fn every_spot_animation_converts() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let all = records.ids(RecordType::SpotAnim).unwrap();
    let out = run::spotanims(&records, 0, None).unwrap();
    let pack = |k: Kind| {
        let w = out.packs.iter().find(|p| p.kind() == k).unwrap();
        PackReader::from_bytes(&w.bake().1).unwrap()
    };
    let (models, rigs, anims) = (pack(Kind::Models), pack(Kind::Rigs), pack(Kind::Anims));
    let table = models.manifest.extra["spotanims"]
        .as_object()
        .unwrap()
        .clone();

    // Every one has a model and an entry; the animated ones a rig and a
    // clip that decode.
    assert_eq!(table.len(), all.len(), "nothing skipped");
    let mut scaled = 0;
    for &s in &all {
        let model = ids::SPOTANIM_MODEL | s;
        assert!(models.contains(model), "spotanim {s} model");
        let e = &table[&model.to_string()];
        assert_eq!(e["spotanim"], s);
        if let Some(clip) = e["clip"].as_u64() {
            let rig = ids::SPOTANIM_RIG | s;
            assert_eq!(e["rig"], rig);
            assert!(rigs.contains(rig));
            let c = Clip::decode(&anims.get(clip as u32).unwrap().unwrap()).unwrap();
            assert_eq!(c.rig, rig);
            scaled += usize::from(c.keys.iter().any(|k| k.scale != [SCALE_ONE; 3]));
        } else {
            assert!(e["rig"].is_null());
        }
    }
    // Spot effects grow and shrink, and the keys say so; and most clips
    // stay under the validation threshold.
    let clips = out.creatures.len();
    let flagged = out.creatures.iter().filter(|(_, c)| c.flagged()).count();
    eprintln!("{clips} clips, {scaled} with scale keys, {flagged} flagged");
    assert!(scaled > 200, "{scaled}");
    assert!(flagged * 100 < clips * 12, "{flagged} of {clips} flagged");

    // Deterministic.
    let again = run::spotanims(&records, 0, None).unwrap();
    for (a, b) in out.packs.iter().zip(&again.packs) {
        assert_eq!(a.bake().0, b.bake().0);
    }
}
