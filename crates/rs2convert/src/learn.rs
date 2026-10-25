//! Learning what structures and rooms are for, and which structures belong
//! to one site, from labelled structures: forests of decision trees
//! ([`crate::tree`]) over features of each structure, each room and each
//! pair of neighbouring structures.
//!
//! The features include the hand-written rules' own scores
//! ([`crate::purpose::class_scores`], [`crate::purpose::room_scores`]), so
//! a tree can lean on a rule where it's right and overrule it where the
//! labels say otherwise. A site is a group of neighbouring structures that
//! belong together (a castle's blocks, a farm's barns): the site tree
//! decides each pair of neighbours, and the groups are what the pairs join.
//! `docs/conversion/structure-purposes.md` has the method and the scores.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::furnish::{self, Category, Hint};
use crate::material::Material;
use crate::purpose::{self, Class, Use};
use crate::structure::{self, Structure};
use crate::tree::{self, Forest, ForestParams, Params, Sample};

/// How far apart, in tiles, two structures can be and still be neighbours.
pub const NEIGHBOUR_GAP: u32 = 24;

/// How many of its nearest neighbours a structure has, at most.
pub const NEIGHBOURS: usize = 12;

/// The rules' classes, in the order [`purpose::class_scores`] gives them.
const RULE_CLASSES: [Class; 19] = [
    Class::Bank,
    Class::Church,
    Class::Tavern,
    Class::Smithy,
    Class::Shop,
    Class::Castle,
    Class::Hall,
    Class::MagicTower,
    Class::Library,
    Class::Workshop,
    Class::Mill,
    Class::Ship,
    Class::Prison,
    Class::Crypt,
    Class::Ruin,
    Class::Farm,
    Class::Garden,
    Class::House,
    Class::Shack,
];

/// The room uses the rules score.
const RULE_USES: [Use; 21] = [
    Use::BankHall,
    Use::Bar,
    Use::Forge,
    Use::Chapel,
    Use::ThroneRoom,
    Use::ShopFloor,
    Use::Workshop,
    Use::Bedroom,
    Use::Kitchen,
    Use::Library,
    Use::Cell,
    Use::Armoury,
    Use::Dining,
    Use::Bathroom,
    Use::Storeroom,
    Use::Garden,
    Use::Pen,
    Use::Courtyard,
    Use::Terrace,
    Use::Stairwell,
    Use::Hall,
];

/// The wall and floor materials whose shares are features.
const WALLS: [Material; 5] = [
    Material::Stone,
    Material::Wood,
    Material::Plain,
    Material::Sandstone,
    Material::Metal,
];
const FLOORS: [Material; 8] = [
    Material::Wood,
    Material::Stone,
    Material::Mud,
    Material::Cobbles,
    Material::Plain,
    Material::Invisible,
    Material::Sand,
    Material::Marble,
];

/// Tile y from which the map's underground areas lie, mostly: they're laid
/// out far north of the surface.
const UNDERGROUND_Y: u16 = 6400;

/// A name for each structure feature, in order.
pub fn structure_feature_names() -> Vec<String> {
    let mut n: Vec<String> = Category::ALL
        .iter()
        .map(|c| format!("{c:?} count").to_lowercase())
        .collect();
    for h in [
        "shop", "bank", "pub", "smithy", "worship", "cooking", "craft", "water", "service",
    ] {
        n.push(format!("{h} icons"));
    }
    for s in [
        "planes",
        "lowest plane",
        "roofed rooms",
        "yards",
        "upper yards",
        "roofed tiles",
        "yard tiles",
        "width",
        "depth",
        "aspect",
        "roundness",
        "doors",
        "doors out",
        "openings",
        "walls",
        "roof pieces",
        "furnishings",
        "unnamed objects",
        "underground",
    ] {
        n.push(s.to_string());
    }
    n.extend(WALLS.iter().map(|m| format!("{m:?} walls").to_lowercase()));
    n.extend(
        FLOORS
            .iter()
            .map(|m| format!("{m:?} floors").to_lowercase()),
    );
    n.extend(
        RULE_CLASSES
            .iter()
            .map(|c| format!("rule {c:?}").to_lowercase()),
    );
    n
}

/// Shares of `of` among `items` for each of `kinds`.
fn shares<T: PartialEq>(items: &[T], kinds: &[T]) -> Vec<f64> {
    let total = items.len().max(1) as f64;
    kinds
        .iter()
        .map(|k| items.iter().filter(|i| *i == k).count() as f64 / total)
        .collect()
}

/// A structure's features, in the order of [`structure_feature_names`].
pub fn structure_features(s: &Structure) -> Vec<f64> {
    let count = |c: Category| s.objects.iter().filter(|o| o.category == Some(c)).count() as f64;
    let mut x: Vec<f64> = Category::ALL.iter().map(|&c| count(c)).collect();
    let hints: Vec<Hint> = s
        .objects
        .iter()
        .filter_map(|o| o.icon)
        .filter_map(|i| furnish::icon(i).and_then(|(_, h)| h))
        .collect();
    let hint = |f: &dyn Fn(&Hint) -> bool| hints.iter().filter(|h| f(h)).count() as f64;
    x.push(hint(&|h| matches!(h, Hint::Shop(_))));
    x.push(hint(&|h| *h == Hint::Bank));
    x.push(hint(&|h| *h == Hint::Tavern));
    x.push(hint(&|h| *h == Hint::Smithy));
    x.push(hint(&|h| *h == Hint::Worship));
    x.push(hint(&|h| *h == Hint::Cooking));
    x.push(hint(&|h| matches!(h, Hint::Spinning | Hint::Pottery)));
    x.push(hint(&|h| *h == Hint::Water));
    x.push(hint(&|h| matches!(h, Hint::Service(_))));
    let roofed: Vec<_> = s.rooms.iter().filter(|r| r.roofed).collect();
    let yards: Vec<_> = s.rooms.iter().filter(|r| !r.roofed).collect();
    let (w, d) = (f64::from(s.size.0), f64::from(s.size.1));
    let walls = s.walls.len().max(1) as f64;
    x.extend([
        s.planes.len() as f64,
        f64::from(s.planes.first().copied().unwrap_or(0)),
        roofed.len() as f64,
        yards.len() as f64,
        yards.iter().filter(|r| r.plane > 0).count() as f64,
        roofed.iter().map(|r| r.tiles.len()).sum::<usize>() as f64,
        yards.iter().map(|r| r.tiles.len()).sum::<usize>() as f64,
        w,
        d,
        w.max(d) / w.min(d).max(1.0),
        s.walls.iter().filter(|p| p.kind == 9).count() as f64 / walls,
        s.doors.len() as f64,
        s.doors
            .iter()
            .filter(|d| !d.open && d.rooms.contains(&None))
            .count() as f64,
        s.openings.len() as f64,
        s.walls.len() as f64,
        s.roofs.len() as f64,
        s.objects
            .iter()
            .filter(|o| matches!(o.kind, 10 | 11))
            .count() as f64,
        s.objects.iter().filter(|o| o.name.is_none()).count() as f64,
        f64::from(u8::from(s.origin.1 >= UNDERGROUND_Y)),
    ]);
    let wall_materials: Vec<Material> = s
        .walls
        .iter()
        .filter(|p| matches!(p.kind, 0 | 2))
        .filter_map(|p| p.material.map(|m| m.material))
        .collect();
    x.extend(shares(&wall_materials, &WALLS));
    let floors: Vec<Material> = s
        .rooms
        .iter()
        .filter_map(|r| r.floor.map(|f| f.material))
        .collect();
    x.extend(shares(&floors, &FLOORS));
    let scores: BTreeMap<Class, u32> = purpose::class_scores(s)
        .into_iter()
        .map(|g| (g.what, g.score))
        .collect();
    x.extend(
        RULE_CLASSES
            .iter()
            .map(|c| f64::from(scores.get(c).copied().unwrap_or(0))),
    );
    x
}

/// A name for each room feature, in order.
pub fn room_feature_names() -> Vec<String> {
    let mut n: Vec<String> = Category::ALL
        .iter()
        .map(|c| format!("{c:?} count").to_lowercase())
        .collect();
    for s in [
        "tiles",
        "roofed",
        "plane",
        "furnishings",
        "unnamed objects",
        "icons",
        "share of structure",
    ] {
        n.push(s.to_string());
    }
    n.extend(
        RULE_USES
            .iter()
            .map(|u| format!("rule {u:?}").to_lowercase()),
    );
    n
}

/// Room `i` of `s`'s features, in the order of [`room_feature_names`].
pub fn room_features(s: &Structure, i: usize) -> Vec<f64> {
    let r = &s.rooms[i];
    let inside: Vec<_> = s.objects.iter().filter(|o| o.room == Some(i)).collect();
    let count = |c: Category| inside.iter().filter(|o| o.category == Some(c)).count() as f64;
    let mut x: Vec<f64> = Category::ALL.iter().map(|&c| count(c)).collect();
    let all_tiles: usize = s.rooms.iter().map(|r| r.tiles.len()).sum();
    x.extend([
        r.tiles.len() as f64,
        f64::from(u8::from(r.roofed)),
        f64::from(r.plane),
        inside.iter().filter(|o| matches!(o.kind, 10 | 11)).count() as f64,
        inside.iter().filter(|o| o.name.is_none()).count() as f64,
        inside.iter().filter(|o| o.icon.is_some()).count() as f64,
        r.tiles.len() as f64 / all_tiles.max(1) as f64,
    ]);
    let scores: BTreeMap<Use, u32> = purpose::room_scores(r, inside.into_iter())
        .into_iter()
        .map(|g| (g.what, g.score))
        .collect();
    x.extend(
        RULE_USES
            .iter()
            .map(|u| f64::from(scores.get(u).copied().unwrap_or(0))),
    );
    x
}

/// The names of the pair features, in order.
pub const PAIR_FEATURES: [&str; 14] = [
    "gap",
    "smaller area",
    "larger area",
    "area ratio",
    "fewer planes",
    "more planes",
    "both have upper yards",
    "same main wall material",
    "shared wall styles",
    "same main floor",
    "same rule class",
    "smaller castle score",
    "overlap east-west",
    "overlap north-south",
];

/// The definition ids of a structure's walls.
fn wall_styles(s: &Structure) -> BTreeSet<u16> {
    s.walls.iter().map(|w| w.id).collect()
}

/// The commonest of some values, ties to the least.
fn commonest<T: Ord + Copy>(items: impl Iterator<Item = T>) -> Option<T> {
    let mut c: BTreeMap<T, usize> = BTreeMap::new();
    for i in items {
        *c.entry(i).or_default() += 1;
    }
    c.into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(k, _)| k)
}

/// The features of a pair of neighbouring structures, in the order of
/// [`PAIR_FEATURES`]. Symmetric: the order of `a` and `b` doesn't matter.
pub fn pair_features(a: &Structure, b: &Structure) -> Vec<f64> {
    let area = |s: &Structure| s.rooms.iter().map(|r| r.tiles.len()).sum::<usize>() as f64;
    let (aa, ab) = (area(a), area(b));
    let upper = |s: &Structure| s.rooms.iter().any(|r| !r.roofed && r.plane > 0);
    let main_wall = |s: &Structure| {
        commonest(
            s.walls
                .iter()
                .filter(|p| matches!(p.kind, 0 | 2))
                .filter_map(|p| p.material.map(|m| m.material)),
        )
    };
    let main_floor =
        |s: &Structure| commonest(s.rooms.iter().filter_map(|r| r.floor.map(|f| f.material)));
    let (sa, sb) = (wall_styles(a), wall_styles(b));
    let union = sa.union(&sb).count().max(1) as f64;
    let rule = |s: &Structure| purpose::of(s).class.what;
    let castle = |s: &Structure| {
        purpose::class_scores(s)
            .iter()
            .find(|g| g.what == Class::Castle)
            .map_or(0, |g| g.score)
    };
    let overlap = |a0: u16, an: u16, b0: u16, bn: u16| {
        let (a1, b1) = (u32::from(a0) + u32::from(an), u32::from(b0) + u32::from(bn));
        let lo = u32::from(a0).max(u32::from(b0));
        f64::from(a1.min(b1).saturating_sub(lo))
    };
    vec![
        f64::from(structure::gap(a, b)),
        aa.min(ab),
        aa.max(ab),
        aa.min(ab) / aa.max(ab).max(1.0),
        a.planes.len().min(b.planes.len()) as f64,
        a.planes.len().max(b.planes.len()) as f64,
        f64::from(u8::from(upper(a) && upper(b))),
        f64::from(u8::from(
            main_wall(a).is_some() && main_wall(a) == main_wall(b),
        )),
        sa.intersection(&sb).count() as f64 / union,
        f64::from(u8::from(
            main_floor(a).is_some() && main_floor(a) == main_floor(b),
        )),
        f64::from(u8::from(rule(a) == rule(b) && rule(a) != Class::Unknown)),
        f64::from(castle(a).min(castle(b))),
        overlap(a.origin.0, a.size.0, b.origin.0, b.size.0),
        overlap(a.origin.1, a.size.1, b.origin.1, b.size.1),
    ]
}

/// Each structure's neighbours: the [`NEIGHBOURS`] nearest within
/// [`NEIGHBOUR_GAP`] tiles, nearest first (ties by id), as indices into
/// `structures`.
pub fn neighbours(structures: &[Structure]) -> Vec<Vec<usize>> {
    let ids: Vec<String> = structures.iter().map(structure::id).collect();
    structures
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut near: Vec<(u32, &str, usize)> = structures
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(j, n)| (structure::gap(s, n), ids[j].as_str(), j))
                .filter(|&(g, _, _)| g <= NEIGHBOUR_GAP)
                .collect();
            near.sort();
            near.truncate(NEIGHBOURS);
            near.into_iter().map(|(_, _, j)| j).collect()
        })
        .collect()
}

/// Every pair of neighbours, each once, lower index first.
fn pairs(near: &[Vec<usize>]) -> BTreeSet<(usize, usize)> {
    near.iter()
        .enumerate()
        .flat_map(|(i, ns)| ns.iter().map(move |&j| (i.min(j), i.max(j))))
        .collect()
}

/// One structure's label, as a labels file holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    /// The structure's id ([`structure::id`]).
    pub id: String,
    /// What it is.
    pub class: Class,
    /// For a shop, what it sells; for a workshop, its craft.
    #[serde(default)]
    pub detail: Option<String>,
    /// What each room is for, by room number.
    #[serde(default)]
    pub rooms: Vec<RoomLabel>,
    /// The neighbours on the same site, by id.
    #[serde(default)]
    pub site: Vec<String>,
    /// `high`, `medium` or `low`.
    #[serde(default)]
    pub confidence: Option<String>,
}

/// One room's label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomLabel {
    /// The room's number in its structure.
    pub room: usize,
    /// What it's for.
    #[serde(rename = "use")]
    pub use_: Use,
}

/// How much a label counts by its confidence: a low-confidence one half.
fn weight(l: &Label) -> f64 {
    if l.confidence.as_deref() == Some("low") {
        0.5
    } else {
        1.0
    }
}

/// The forests learned from labels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trees {
    /// What a structure is, from [`structure_features`].
    pub class: Forest<Class>,
    /// What a room is for, from [`room_features`].
    pub room: Forest<Use>,
    /// Whether two neighbours are on one site, from [`pair_features`].
    pub site: Forest<bool>,
}

/// The trees the tools come with, learned on the maintainer's cache with
/// `rs2ool cache structures --learn --save`.
pub const BUILT_IN: &str = include_str!("../data/structure-trees.json");

impl Trees {
    /// [`BUILT_IN`], parsed.
    pub fn built_in() -> Self {
        serde_json::from_str(BUILT_IN).expect("the built-in structure trees parse")
    }

    /// What `s` is, and what each of its rooms is for.
    pub fn classify(&self, s: &Structure) -> (Class, Vec<Use>) {
        let class = self.class.predict(&structure_features(s), &Class::Unknown);
        let rooms = (0..s.rooms.len())
            .map(|i| self.room.predict(&room_features(s, i), &Use::Unknown))
            .collect();
        (class, rooms)
    }

    /// The sites `structures` fall into: each structure's site number, the
    /// lowest index of a structure in the group the predicted pairs join.
    pub fn sites(&self, structures: &[Structure]) -> Vec<usize> {
        let near = neighbours(structures);
        let mut site: Vec<usize> = (0..structures.len()).collect();
        for (a, b) in pairs(&near) {
            if self
                .site
                .predict(&pair_features(&structures[a], &structures[b]), &false)
            {
                join(&mut site, a, b);
            }
        }
        (0..structures.len()).map(|i| find(&mut site, i)).collect()
    }
}

fn find(site: &mut [usize], i: usize) -> usize {
    let mut r = i;
    while site[r] != r {
        r = site[r];
    }
    site[i] = r;
    r
}

fn join(site: &mut [usize], a: usize, b: usize) {
    let (a, b) = (find(site, a), find(site, b));
    site[a.max(b)] = a.min(b);
}

/// How well something was guessed: right out of all, for the learned trees
/// (held out) and the rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    /// Labels.
    pub total: usize,
    /// Right by the learned tree, held out.
    pub learned: usize,
    /// Right by the rules.
    pub rules: usize,
}

/// How well the site tree did, held out, over neighbouring pairs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteScore {
    /// Pairs.
    pub pairs: usize,
    /// Pairs labelled as one site.
    pub labelled: usize,
    /// Pairs predicted as one site that are.
    pub right: usize,
    /// Pairs predicted as one site that aren't.
    pub wrong: usize,
}

/// What learning found: the trees, grown on every label, and how they do
/// held out.
#[derive(Debug, Clone, PartialEq)]
pub struct Learned {
    /// The trees.
    pub trees: Trees,
    /// Structure classes.
    pub classes: Score,
    /// Room uses.
    pub rooms: Score,
    /// Sites.
    pub sites: SiteScore,
    /// Each labelled structure's class: labelled, held-out learned, rules.
    pub class_table: Vec<(Class, Class, Class)>,
}

/// The forest sizes to learn with: classes, rooms and sites.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sizes {
    /// For classes.
    pub class: ForestParams,
    /// For room uses.
    pub room: ForestParams,
    /// For sites.
    pub site: ForestParams,
}

/// The size of every forest by default: 50 trees, each up to 16 deep with
/// leaves of any weight, trying the square root of the features at each
/// split.
pub const FOREST: ForestParams = ForestParams {
    trees: 50,
    tree: Params {
        max_depth: 16,
        min_leaf: 1.0,
    },
    features: None,
};

impl Default for Sizes {
    fn default() -> Self {
        Self {
            class: FOREST,
            room: FOREST,
            site: FOREST,
        }
    }
}

/// How many groups cross-validation holds out in turn.
pub const FOLDS: usize = 10;

/// Learns from `labels` on `structures` (labels whose id matches no
/// structure are skipped), scoring each tree by [`FOLDS`]-fold
/// cross-validation, held out by structure, against the rules.
pub fn learn(structures: &[Structure], labels: &[Label], sizes: Sizes) -> Learned {
    let index: BTreeMap<String, usize> = structures
        .iter()
        .enumerate()
        .map(|(i, s)| (structure::id(s), i))
        .collect();
    let labelled: Vec<(usize, &Label)> = labels
        .iter()
        .filter_map(|l| Some((*index.get(&l.id)?, l)))
        .collect();
    // Structures are held out in folds by their place in the label list.
    let fold_of: BTreeMap<usize, usize> = labelled
        .iter()
        .enumerate()
        .map(|(k, &(i, _))| (i, k))
        .collect();

    // Classes.
    let class_samples: Vec<Sample<Class>> = labelled
        .iter()
        .map(|&(i, l)| Sample {
            x: structure_features(&structures[i]),
            label: l.class,
            weight: weight(l),
        })
        .collect();
    let held =
        tree::cross_validate_forest(&class_samples, &|k| k, FOLDS, sizes.class, &Class::Unknown);
    let mut class_table = Vec::new();
    let mut classes = Score::default();
    for (&(i, l), learned) in labelled.iter().zip(&held) {
        let rules = purpose::of(&structures[i]).class.what;
        classes.total += 1;
        classes.learned += usize::from(*learned == l.class);
        classes.rules += usize::from(rules == l.class);
        class_table.push((l.class, *learned, rules));
    }

    // Rooms, held out with their structure.
    let mut room_samples: Vec<Sample<Use>> = Vec::new();
    let mut room_group: Vec<usize> = Vec::new();
    let mut room_rules: Vec<Use> = Vec::new();
    for &(i, l) in &labelled {
        let s = &structures[i];
        let rules = purpose::of(s).rooms;
        for r in &l.rooms {
            if r.room < s.rooms.len() {
                room_samples.push(Sample {
                    x: room_features(s, r.room),
                    label: r.use_,
                    weight: weight(l),
                });
                room_group.push(fold_of[&i]);
                room_rules.push(rules[r.room].what);
            }
        }
    }
    let held = tree::cross_validate_forest(
        &room_samples,
        &|k| room_group[k],
        FOLDS,
        sizes.room,
        &Use::Unknown,
    );
    let mut rooms = Score::default();
    for ((s, learned), rule) in room_samples.iter().zip(&held).zip(&room_rules) {
        rooms.total += 1;
        rooms.learned += usize::from(*learned == s.label);
        rooms.rules += usize::from(*rule == s.label);
    }

    // Sites: every pair of neighbours among the labelled structures.
    let near = neighbours(structures);
    let label_of: BTreeMap<usize, &Label> = labelled.iter().copied().collect();
    let mut site_samples: Vec<Sample<bool>> = Vec::new();
    let mut site_group: Vec<usize> = Vec::new();
    for (a, b) in pairs(&near) {
        let (Some(la), Some(lb)) = (label_of.get(&a), label_of.get(&b)) else {
            continue;
        };
        let (ia, ib) = (structure::id(&structures[a]), structure::id(&structures[b]));
        let together = la.site.contains(&ib) || lb.site.contains(&ia);
        site_samples.push(Sample {
            x: pair_features(&structures[a], &structures[b]),
            label: together,
            weight: weight(la).min(weight(lb)),
        });
        site_group.push(fold_of[&a.min(b)]);
    }
    let held =
        tree::cross_validate_forest(&site_samples, &|k| site_group[k], FOLDS, sizes.site, &false);
    let mut sites = SiteScore::default();
    for (s, predicted) in site_samples.iter().zip(&held) {
        sites.pairs += 1;
        sites.labelled += usize::from(s.label);
        sites.right += usize::from(*predicted && s.label);
        sites.wrong += usize::from(*predicted && !s.label);
    }

    Learned {
        trees: Trees {
            class: Forest::train(&class_samples, sizes.class, Class::Unknown),
            room: Forest::train(&room_samples, sizes.room, Use::Unknown),
            site: Forest::train(&site_samples, sizes.site, false),
        },
        classes,
        rooms,
        sites,
        class_table,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::{Placed, Room, RoomTile};

    /// A one-room structure at `(x, y)`, `w` × `d` tiles, with a bed.
    fn hut(x: u16, y: u16, w: u16, d: u16) -> Structure {
        let tiles = (0..w)
            .flat_map(|i| (0..d).map(move |j| (i, j)))
            .map(|(x, y)| RoomTile {
                x,
                y,
                overlay: 0,
                underlay: 1,
                height: 0,
            })
            .collect();
        Structure {
            origin: (x, y),
            size: (w, d),
            planes: vec![0],
            rooms: vec![Room {
                plane: 0,
                roofed: true,
                tiles,
                floor: None,
            }],
            doors: Vec::new(),
            openings: Vec::new(),
            walls: Vec::new(),
            roofs: Vec::new(),
            objects: vec![Placed {
                id: 1,
                name: Some("Bed".into()),
                category: furnish::category("Bed"),
                icon: None,
                material: None,
                kind: 10,
                rotation: 0,
                plane: 0,
                x: 1,
                y: 1,
                room: Some(0),
            }],
            clipped: false,
        }
    }

    #[test]
    fn features_match_their_names() {
        let s = hut(100, 100, 4, 3);
        assert_eq!(
            structure_features(&s).len(),
            structure_feature_names().len()
        );
        assert_eq!(room_features(&s, 0).len(), room_feature_names().len());
        assert_eq!(pair_features(&s, &s).len(), PAIR_FEATURES.len());
        let names = structure_feature_names();
        let bed = names.iter().position(|n| n == "bed count").unwrap();
        assert_eq!(structure_features(&s)[bed], 1.0);
    }

    #[test]
    fn pairs_are_symmetric_and_neighbours_nearest_first() {
        let (a, b) = (hut(100, 100, 4, 3), hut(110, 100, 6, 6));
        assert_eq!(pair_features(&a, &b), pair_features(&b, &a));
        assert_eq!(pair_features(&a, &b)[0], 6.0, "the gap");
        let far = hut(200, 100, 3, 3);
        let near = neighbours(&[a, b, far]);
        assert_eq!(near, [vec![1], vec![0], vec![]]);
    }

    #[test]
    fn the_built_in_trees_load() {
        let t = Trees::built_in();
        assert!(!t.class.trees.is_empty() && !t.room.trees.is_empty() && !t.site.trees.is_empty());
        // A lone hut gets some class, and its one room some use.
        let (_, rooms) = t.classify(&hut(100, 100, 4, 3));
        assert_eq!(rooms.len(), 1);
    }
}
