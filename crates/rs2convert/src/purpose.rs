//! What a structure and each of its rooms are likely for, worked out from
//! what's in them: furnishing categories ([`crate::furnish`]), minimap
//! icons, and shape (floors, rooms, area, roundness, yards).
//!
//! Every class scores points from its evidence, and each piece of evidence
//! is written down, so a guess can be checked and argued with. The best
//! class needs [`MIN_SCORE`] points; below that the structure is
//! [`Class::Unknown`]. Runners-up with enough points come along as
//! alternatives. These are guesses, never cache facts: the map doesn't say
//! what a building is.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::furnish::{self, Category, Hint};
use crate::structure::{Placed, Room, Structure};

/// The fewest points a guess needs to be made.
pub const MIN_SCORE: u32 = 3;

/// What a structure is likely to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// A home: beds, a kitchen, tables.
    House,
    /// A small, bare, one-room hut.
    Shack,
    /// A pub: a bar, or a beer-mug icon.
    Tavern,
    /// A smithy: an anvil or a furnace.
    Smithy,
    /// A shop: a counter or goods on show, or a shop icon (which says what
    /// it sells).
    Shop,
    /// A bank: bank booths, or the bank icon.
    Bank,
    /// A church or temple: an altar, pews, an organ.
    Church,
    /// A castle or keep: big, several floors, a throne or arms.
    Castle,
    /// A hall for gatherings: big, a long table, many seats.
    Hall,
    /// A tall, round tower with magic in it.
    MagicTower,
    /// A library or study: many books.
    Library,
    /// A workshop for a craft: spinning, pottery, tanning, weaving.
    Workshop,
    /// A mill: hoppers and millstones.
    Mill,
    /// A ship, or a deck's worth of ship's fittings.
    Ship,
    /// A prison: cages and stocks.
    Prison,
    /// A crypt or graveyard: graves and coffins.
    Crypt,
    /// A ruin: decay, broken furniture, no roof.
    Ruin,
    /// A pen or farmyard: a yard with hay and troughs.
    Farm,
    /// A garden: a yard of plants.
    Garden,
    /// A civic building. Labels only: no rule gives it.
    TownHall,
    /// An underground or natural place: a cave, a dungeon, a mine, a
    /// tunnel. Labels only.
    Cave,
    /// A tower that's neither a magic tower nor part of a castle: a
    /// watchtower, a lookout, a stair tower. Labels only.
    Tower,
    /// A walled or fenced open area with little built in it: a pen, a
    /// walled plot, a compound, an arena. Labels only.
    Enclosure,
    /// A market stall, a tent, a booth or a small open-sided shelter.
    /// Labels only.
    Stall,
    /// Sleeping quarters for many: rows of beds, a dormitory, a
    /// guardhouse. Labels only.
    Barracks,
    /// A building given over to storage: a warehouse, a granary, a shed
    /// of stores. Labels only.
    Storehouse,
    /// Something none of these fits, by a labeller's judgement. Labels
    /// only.
    Other,
    /// Nothing scored enough.
    Unknown,
}

/// What a room is likely to be for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Use {
    /// Beds.
    Bedroom,
    /// Cooking.
    Kitchen,
    /// A table with seats round it.
    Dining,
    /// A bar.
    Bar,
    /// Bank booths.
    BankHall,
    /// A counter or goods on show.
    ShopFloor,
    /// An anvil or a furnace.
    Forge,
    /// A craft's tools.
    Workshop,
    /// An altar or pews.
    Chapel,
    /// Books.
    Library,
    /// A throne.
    ThroneRoom,
    /// Mostly storage.
    Storeroom,
    /// Washing, and no cooking.
    Bathroom,
    /// Arms and defence.
    Armoury,
    /// Cages or stocks.
    Cell,
    /// Little but a way up or down.
    Stairwell,
    /// Big and sparsely furnished.
    Hall,
    /// A yard on the ground, part of a building.
    Courtyard,
    /// A yard above the ground: a roof terrace.
    Terrace,
    /// A yard of plants.
    Garden,
    /// A yard with hay or troughs.
    Pen,
    /// Nothing in it.
    Empty,
    /// A room for sitting and living in. Labels only: no rule gives it.
    LivingRoom,
    /// A study. Labels only.
    Study,
    /// An office. Labels only.
    Office,
    /// A corridor or passage. Labels only.
    Corridor,
    /// A cave or tunnel. Labels only.
    Cave,
    /// Something none of these fits, by a labeller's judgement. Labels
    /// only.
    Other,
    /// Something in it, but nothing that says what for.
    Unknown,
}

/// One guess, with its score and the evidence for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guess<T> {
    /// What it's guessed to be.
    pub what: T,
    /// For a shop, what it sells, from its icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Its points.
    pub score: u32,
    /// What the points are for, one line each.
    pub evidence: Vec<String>,
}

/// A structure's guess and its rooms' guesses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Purpose {
    /// The best guess for the structure.
    pub class: Guess<Class>,
    /// Other classes with at least [`MIN_SCORE`] points, best first.
    pub alternatives: Vec<Guess<Class>>,
    /// The best guess for each room, in the structure's room order.
    pub rooms: Vec<Guess<Use>>,
}

/// Counts of furnishing categories, and the icons, among some objects.
#[derive(Debug, Default)]
struct Tally {
    categories: BTreeMap<Category, u32>,
    icons: Vec<u16>,
    furnishings: u32,
}

impl Tally {
    fn of<'a>(objects: impl Iterator<Item = &'a Placed>) -> Self {
        let mut t = Self::default();
        for o in objects {
            if let Some(i) = o.icon {
                t.icons.push(i);
            }
            if matches!(o.kind, 10 | 11) {
                t.furnishings += 1;
            }
            if let Some(c) = o.category {
                *t.categories.entry(c).or_default() += 1;
            }
        }
        t
    }

    fn n(&self, c: Category) -> u32 {
        self.categories.get(&c).copied().unwrap_or(0)
    }

    /// The icons' hints, with what each shows.
    fn hints(&self) -> impl Iterator<Item = (&'static str, Hint)> + '_ {
        self.icons
            .iter()
            .filter_map(|&i| furnish::icon(i))
            .filter_map(|(shows, hint)| Some((shows, hint?)))
    }
}

/// Collects points and evidence for one guess.
struct Score<T> {
    what: T,
    detail: Option<String>,
    score: u32,
    evidence: Vec<String>,
}

impl<T> Score<T> {
    fn new(what: T) -> Self {
        Self {
            what,
            detail: None,
            score: 0,
            evidence: Vec::new(),
        }
    }

    /// `points` for each of `n` (up to `cap` points in all), noted as
    /// `n what`.
    fn per(&mut self, n: u32, points: u32, cap: u32, what: &str) -> &mut Self {
        if n > 0 {
            self.score += (n * points).min(cap);
            self.evidence.push(format!("{n} × {what}"));
        }
        self
    }

    /// `points` when `cond` holds, noted as `why`.
    fn when(&mut self, cond: bool, points: u32, why: impl Into<String>) -> &mut Self {
        if cond {
            self.score += points;
            self.evidence.push(why.into());
        }
        self
    }

    fn done(self) -> Guess<T> {
        Guess {
            what: self.what,
            detail: self.detail,
            score: self.score,
            evidence: self.evidence,
        }
    }
}

/// Shape facts about a structure.
struct Shape {
    planes: usize,
    roofed_rooms: usize,
    yards: usize,
    area: usize,
    /// The share of its walls that are diagonal: high for a round tower.
    round: f64,
}

impl Shape {
    fn of(s: &Structure) -> Self {
        let walls = s.walls.len().max(1);
        let diagonal = s.walls.iter().filter(|w| w.kind == 9).count();
        Self {
            planes: s.planes.len(),
            roofed_rooms: s.rooms.iter().filter(|r| r.roofed).count(),
            yards: s.rooms.iter().filter(|r| !r.roofed).count(),
            area: s
                .rooms
                .iter()
                .filter(|r| r.roofed)
                .map(|r| r.tiles.len())
                .sum(),
            round: diagonal as f64 / walls as f64,
        }
    }
}

/// Every class the rules know, scored for `s`, in the order they're
/// tried: from the most telling evidence to the least.
pub fn class_scores(s: &Structure) -> Vec<Guess<Class>> {
    let t = Tally::of(s.objects.iter());
    let shape = Shape::of(s);
    vec![
        bank(&t),
        church(&t),
        tavern(&t),
        smithy(&t),
        shop(&t),
        castle(&t, &shape),
        hall(&t, &shape),
        magic_tower(&t, &shape),
        library(&t),
        workshop(&t),
        mill(&t),
        ship(&t),
        prison(&t),
        crypt(&t),
        ruin(&t, &shape),
        farm(&t, &shape),
        garden(&t, &shape),
        house(&t, &shape),
        shack(&t, &shape),
    ]
}

/// Works out what `s` and its rooms are likely for.
pub fn of(s: &Structure) -> Purpose {
    let mut guesses = class_scores(s);
    // Best first; ties go to the earlier class, which the rules try in
    // order from the most telling evidence to the least.
    guesses.retain(|g| g.score >= MIN_SCORE);
    guesses.sort_by_key(|g| std::cmp::Reverse(g.score));
    let class = if guesses.is_empty() {
        Guess {
            what: Class::Unknown,
            detail: None,
            score: 0,
            evidence: Vec::new(),
        }
    } else {
        guesses.remove(0)
    };
    Purpose {
        class,
        alternatives: guesses,
        rooms: s
            .rooms
            .iter()
            .enumerate()
            .map(|(i, r)| room(r, s.objects.iter().filter(|o| o.room == Some(i))))
            .collect(),
    }
}

fn bank(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Bank);
    g.per(t.n(Category::Bank), 5, 20, "bank furniture");
    for (shows, hint) in t.hints() {
        g.when(hint == Hint::Bank, 10, format!("minimap icon: {shows}"));
    }
    g.done()
}

fn church(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Church);
    g.per(t.n(Category::Worship), 3, 15, "altar, pew, organ or font");
    for (shows, hint) in t.hints() {
        g.when(hint == Hint::Worship, 6, format!("minimap icon: {shows}"));
    }
    g.done()
}

fn tavern(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Tavern);
    g.per(t.n(Category::Bar), 4, 16, "bar, pumps or bar stool");
    for (shows, hint) in t.hints() {
        g.when(hint == Hint::Tavern, 10, format!("minimap icon: {shows}"));
    }
    g.done()
}

fn smithy(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Smithy);
    g.per(
        t.n(Category::Smithing),
        4,
        16,
        "anvil, furnace or smith's tools",
    );
    for (shows, hint) in t.hints() {
        g.when(hint == Hint::Smithy, 8, format!("minimap icon: {shows}"));
    }
    g.done()
}

fn shop(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Shop);
    g.per(t.n(Category::Counter), 4, 12, "counter or stall");
    g.per(t.n(Category::Display), 2, 8, "goods on show");
    for (shows, hint) in t.hints() {
        if let Hint::Shop(sells) = hint {
            g.when(true, 10, format!("minimap icon: {shows}"));
            g.detail.get_or_insert_with(|| sells.to_string());
        }
    }
    g.done()
}

fn castle(t: &Tally, s: &Shape) -> Guess<Class> {
    let mut g = Score::new(Class::Castle);
    g.when(
        s.planes >= 2 && s.roofed_rooms >= 6,
        4,
        format!("{} floors, {} rooms", s.planes, s.roofed_rooms),
    );
    g.when(s.area >= 300, 3, format!("{} roofed tiles", s.area));
    g.per(t.n(Category::Throne), 6, 6, "throne");
    g.per(t.n(Category::Military), 1, 6, "arms or defence");
    g.when(
        s.yards > 0 && s.planes >= 2,
        1,
        "walled yard or roof terrace",
    );
    g.done()
}

fn hall(t: &Tally, s: &Shape) -> Guess<Class> {
    let mut g = Score::new(Class::Hall);
    let seats = t.n(Category::Seat);
    g.when(
        s.area >= 120 && seats >= 8 && t.n(Category::Table) >= 1,
        5,
        format!("{seats} seats round tables in {} tiles", s.area),
    );
    g.done()
}

fn magic_tower(t: &Tally, s: &Shape) -> Guess<Class> {
    let mut g = Score::new(Class::MagicTower);
    g.when(s.planes >= 3, 3, format!("{} floors", s.planes));
    g.when(
        s.round >= 0.25,
        3,
        format!("round: {:.0}% of its walls are diagonal", s.round * 100.0),
    );
    g.per(t.n(Category::Magic), 3, 9, "magic object");
    g.per(t.n(Category::Books), 1, 3, "bookcase or study desk");
    // It takes both: height or roundness alone make a tower, not a magic
    // one, and magic alone a magic room, not a tower.
    let tower = s.planes >= 3 || s.round >= 0.25;
    if t.n(Category::Magic) == 0 || !tower {
        g.score = g.score.min(MIN_SCORE - 1);
    }
    g.done()
}

fn library(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Library);
    let books = t.n(Category::Books);
    g.when(
        books >= 4,
        books.min(10),
        format!("{books} × bookcase or study desk"),
    );
    g.done()
}

fn workshop(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Workshop);
    g.per(t.n(Category::Crafting), 3, 12, "craft's tools");
    for (shows, hint) in t.hints() {
        g.when(
            matches!(hint, Hint::Spinning | Hint::Pottery),
            6,
            format!("minimap icon: {shows}"),
        );
    }
    g.done()
}

fn mill(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Mill);
    g.per(t.n(Category::Mill), 5, 15, "hopper or millstones");
    g.done()
}

fn ship(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Ship);
    g.per(t.n(Category::Ship), 2, 12, "ship's fitting");
    g.done()
}

fn prison(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Prison);
    g.per(t.n(Category::Prison), 3, 12, "cage, stocks or chains");
    g.done()
}

fn crypt(t: &Tally) -> Guess<Class> {
    let mut g = Score::new(Class::Crypt);
    g.per(t.n(Category::Grave), 2, 12, "grave or coffin");
    g.done()
}

fn ruin(t: &Tally, s: &Shape) -> Guess<Class> {
    let mut g = Score::new(Class::Ruin);
    g.per(t.n(Category::Remains), 2, 10, "remains or broken furniture");
    g.when(
        s.roofed_rooms == 0 && t.n(Category::Remains) > 0,
        3,
        "no roof",
    );
    g.done()
}

fn farm(t: &Tally, s: &Shape) -> Guess<Class> {
    let mut g = Score::new(Class::Farm);
    g.per(t.n(Category::Farm), 2, 10, "hay, trough, coop or crop");
    g.when(
        s.roofed_rooms == 0 && t.n(Category::Farm) > 0,
        2,
        "an open yard",
    );
    g.done()
}

fn garden(t: &Tally, s: &Shape) -> Guess<Class> {
    let mut g = Score::new(Class::Garden);
    let plants = t.n(Category::Plant);
    g.when(
        s.roofed_rooms == 0 && plants >= 3,
        plants.min(8),
        format!("a yard with {plants} plants"),
    );
    g.done()
}

fn house(t: &Tally, s: &Shape) -> Guess<Class> {
    let mut g = Score::new(Class::House);
    g.per(t.n(Category::Bed), 2, 6, "bed");
    g.per(t.n(Category::Cooking), 1, 3, "cooking");
    g.when(t.n(Category::Table) > 0, 1, "a table");
    g.when(
        (1..=6).contains(&s.roofed_rooms) && s.area <= 150 && t.n(Category::Bed) > 0,
        1,
        format!("{} rooms in {} tiles", s.roofed_rooms, s.area),
    );
    g.done()
}

fn shack(t: &Tally, s: &Shape) -> Guess<Class> {
    let mut g = Score::new(Class::Shack);
    g.when(
        s.roofed_rooms == 1 && s.yards == 0 && s.planes == 1 && s.area <= 20 && t.furnishings <= 6,
        4,
        format!(
            "one bare room of {} tiles with {} furnishings",
            s.area, t.furnishings
        ),
    );
    g.done()
}

/// The best guess for one room.
fn room<'a>(r: &Room, objects: impl Iterator<Item = &'a Placed>) -> Guess<Use> {
    let objects: Vec<&Placed> = objects.collect();
    let furnished = objects.iter().any(|o| matches!(o.kind, 10 | 11));
    let mut guesses = room_scores(r, objects.into_iter());
    guesses.retain(|g| g.score >= MIN_SCORE || matches!(g.what, Use::Courtyard | Use::Terrace));
    // Best first, ties to the order the rules try them: from the most
    // telling to the least.
    guesses.sort_by_key(|g| std::cmp::Reverse(g.score));
    guesses.into_iter().next().unwrap_or_else(|| Guess {
        what: if furnished { Use::Unknown } else { Use::Empty },
        detail: None,
        score: 0,
        evidence: Vec::new(),
    })
}

/// Every room use the rules give some points, scored for room `r` with
/// `objects` in it, in the order they're tried.
pub fn room_scores<'a>(r: &Room, objects: impl Iterator<Item = &'a Placed>) -> Vec<Guess<Use>> {
    let t = Tally::of(objects);
    let tiles = r.tiles.len() as u32;
    let n = |c| t.n(c);
    let mut guesses: Vec<Guess<Use>> = Vec::new();
    let mut add = |what: Use, score: u32, why: String| {
        if score > 0 {
            guesses.push(Guess {
                what,
                detail: None,
                score,
                evidence: vec![why],
            });
        }
    };
    add(
        Use::BankHall,
        n(Category::Bank) * 5,
        format!("{} × bank furniture", n(Category::Bank)),
    );
    add(
        Use::Bar,
        n(Category::Bar) * 4,
        format!("{} × bar, pumps or bar stool", n(Category::Bar)),
    );
    add(
        Use::Forge,
        n(Category::Smithing) * 4,
        format!(
            "{} × anvil, furnace or smith's tools",
            n(Category::Smithing)
        ),
    );
    add(
        Use::Chapel,
        n(Category::Worship) * 3,
        format!("{} × altar, pew, organ or font", n(Category::Worship)),
    );
    add(
        Use::ThroneRoom,
        n(Category::Throne) * 6,
        format!("{} × throne", n(Category::Throne)),
    );
    add(
        Use::ShopFloor,
        n(Category::Counter) * 4 + n(Category::Display) * 2,
        format!(
            "{} × counter, {} × goods on show",
            n(Category::Counter),
            n(Category::Display)
        ),
    );
    add(
        Use::Workshop,
        n(Category::Crafting) * 3,
        format!("{} × craft's tools", n(Category::Crafting)),
    );
    add(
        Use::Bedroom,
        n(Category::Bed) * 3,
        format!("{} × bed", n(Category::Bed)),
    );
    add(
        Use::Kitchen,
        n(Category::Cooking) * 3,
        format!("{} × cooking", n(Category::Cooking)),
    );
    add(
        Use::Library,
        if n(Category::Books) >= 2 {
            n(Category::Books) * 2
        } else {
            0
        },
        format!("{} × bookcase or study desk", n(Category::Books)),
    );
    add(
        Use::Cell,
        n(Category::Prison) * 3,
        format!("{} × cage, stocks or chains", n(Category::Prison)),
    );
    add(
        Use::Armoury,
        if n(Category::Military) >= 2 {
            n(Category::Military)
        } else {
            0
        },
        format!("{} × arms or defence", n(Category::Military)),
    );
    add(
        Use::Dining,
        if n(Category::Table) >= 1 && n(Category::Seat) >= 3 {
            2 + n(Category::Seat).min(6)
        } else {
            0
        },
        format!(
            "{} × table, {} × seat",
            n(Category::Table),
            n(Category::Seat)
        ),
    );
    add(
        Use::Bathroom,
        if n(Category::Cooking) == 0 {
            n(Category::Washing) * 3
        } else {
            0
        },
        format!("{} × sink or bath, no cooking", n(Category::Washing)),
    );
    let storage = n(Category::Storage);
    add(
        Use::Storeroom,
        if storage >= 3 && storage * 2 >= t.furnishings {
            storage.min(8)
        } else {
            0
        },
        format!("{storage} of {} furnishings are storage", t.furnishings),
    );
    if !r.roofed {
        add(
            Use::Garden,
            if n(Category::Plant) >= 3 {
                n(Category::Plant).min(8)
            } else {
                0
            },
            format!("{} × plant in the open", n(Category::Plant)),
        );
        add(
            Use::Pen,
            n(Category::Farm) * 2,
            format!(
                "{} × hay, trough, coop or crop in the open",
                n(Category::Farm)
            ),
        );
        let what = if r.plane > 0 {
            Use::Terrace
        } else {
            Use::Courtyard
        };
        add(what, 2, format!("open to the sky on plane {}", r.plane));
    }
    let only_stairs = n(Category::Stairs) > 0 && t.furnishings == n(Category::Stairs);
    add(
        Use::Stairwell,
        if only_stairs && tiles <= 16 { 4 } else { 0 },
        format!(
            "{} × ladder or stairs and nothing else",
            n(Category::Stairs)
        ),
    );
    add(
        Use::Hall,
        if r.roofed && tiles >= 40 && t.furnishings * 8 <= tiles {
            3
        } else {
            0
        },
        format!("{tiles} tiles with {} furnishings", t.furnishings),
    );
    guesses
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::RoomTile;

    /// A room of `n` tiles on `plane`.
    fn room_of(plane: u8, roofed: bool, n: u16) -> Room {
        Room {
            plane,
            roofed,
            floor: None,
            tiles: (0..n)
                .map(|i| RoomTile {
                    x: i % 8,
                    y: i / 8,
                    overlay: 0,
                    underlay: 0,
                    height: 0,
                })
                .collect(),
        }
    }

    /// A furnishing (type 10) named `name` in `room`.
    fn thing(name: &str, room: usize) -> Placed {
        Placed {
            id: 0,
            name: Some(name.into()),
            category: furnish::category(name),
            icon: None,
            material: None,
            kind: 10,
            rotation: 0,
            plane: 0,
            x: 0,
            y: 0,
            room: Some(room),
        }
    }

    /// A minimap icon placed in `room`.
    fn icon(index: u16, room: usize) -> Placed {
        Placed {
            name: None,
            category: None,
            icon: Some(index),
            kind: 22,
            ..thing("", room)
        }
    }

    fn structure(rooms: Vec<Room>, objects: Vec<Placed>) -> Structure {
        let planes = rooms
            .iter()
            .map(|r| r.plane)
            .collect::<std::collections::BTreeSet<_>>();
        Structure {
            origin: (0, 0),
            size: (8, 8),
            planes: planes.into_iter().collect(),
            rooms,
            doors: Vec::new(),
            openings: Vec::new(),
            walls: Vec::new(),
            roofs: Vec::new(),
            objects,
            clipped: false,
        }
    }

    #[test]
    fn a_bank_by_its_booths_and_its_icon() {
        let s = structure(
            vec![room_of(0, true, 40)],
            vec![thing("Bank booth", 0), thing("Bank booth", 0), icon(5, 0)],
        );
        let p = of(&s);
        assert_eq!(p.class.what, Class::Bank);
        assert_eq!(p.class.score, 20);
        assert_eq!(
            p.class.evidence,
            ["2 × bank furniture", "minimap icon: a dollar sign"]
        );
        assert_eq!(p.rooms[0].what, Use::BankHall);
    }

    #[test]
    fn a_shops_icon_says_what_it_sells() {
        let s = structure(
            vec![room_of(0, true, 30)],
            vec![thing("Counter", 0), thing("Shelves", 0), icon(1, 0)],
        );
        let p = of(&s);
        assert_eq!(
            (p.class.what, p.class.detail.as_deref()),
            (Class::Shop, Some("swords"))
        );
    }

    #[test]
    fn a_house_with_a_bedroom_and_a_kitchen() {
        let s = structure(
            vec![room_of(0, true, 20), room_of(0, true, 16)],
            vec![
                thing("Bed", 0),
                thing("Bed", 0),
                thing("Drawers", 0),
                thing("Range", 1),
                thing("Table", 1),
                thing("Sink", 1),
            ],
        );
        let p = of(&s);
        assert_eq!(p.class.what, Class::House);
        let uses: Vec<Use> = p.rooms.iter().map(|r| r.what).collect();
        assert_eq!(uses, [Use::Bedroom, Use::Kitchen]);
    }

    #[test]
    fn a_magic_tower_needs_the_tower_and_the_magic() {
        let mut s = structure(
            vec![
                room_of(0, true, 30),
                room_of(1, true, 30),
                room_of(2, true, 30),
            ],
            vec![thing("Magical symbol", 2), thing("Bookcase", 1)],
        );
        assert_eq!(of(&s).class.what, Class::MagicTower);
        // The same magic on one floor is no tower.
        s.rooms.truncate(1);
        s.planes = vec![0];
        s.objects[0].room = Some(0);
        s.objects[1].room = Some(0);
        assert_ne!(of(&s).class.what, Class::MagicTower);
    }

    #[test]
    fn yards_and_bare_rooms() {
        let s = structure(
            vec![
                room_of(0, true, 12),
                room_of(0, false, 30),
                room_of(1, false, 12),
                room_of(0, true, 9),
            ],
            vec![thing("Ladder", 3)],
        );
        let p = of(&s);
        let uses: Vec<Use> = p.rooms.iter().map(|r| r.what).collect();
        assert_eq!(
            uses,
            [Use::Empty, Use::Courtyard, Use::Terrace, Use::Stairwell]
        );
        // Nothing telling anywhere: no guess.
        assert_eq!(p.class.what, Class::Unknown);
        assert!(p.alternatives.is_empty());
    }
}
