//! What a placed object is, in general terms: its furnishing category,
//! from its name, and what its minimap icon shows.
//!
//! Categories are what lets a blueprint keep its structure apart from its
//! dressing: a bed is a bed whichever bed it is, so one can be swapped for
//! another. They also carry most of the evidence for what a building and
//! its rooms are for (see [`crate::purpose`]).
//!
//! A name falls in the first category one of whose keywords it contains as
//! whole words, ignoring case: "smashed chair" is remains before "chair"
//! makes it a seat, "cooking shelves" is cooking before "shelves" makes it
//! storage, "bar pumps" is the bar before anything else.
//! The keywords are plain words, never proper names. A name that matches
//! nothing has no category.

use serde::{Deserialize, Serialize};

/// A furnishing category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    /// Banking furniture: booths, bank tables, deposit boxes.
    Bank,
    /// A bar: pumps, the bar itself, bar stools, kegs.
    Bar,
    /// Smithing: anvils, furnaces, a smith's tools.
    Smithing,
    /// Crafts and trades: spinning wheels, potter's wheels and kilns, looms,
    /// tanning, vats, churns, workbenches.
    Crafting,
    /// Worship: altars, pews, organs, fonts.
    Worship,
    /// A throne.
    Throne,
    /// A shop or market counter, or a stall.
    Counter,
    /// Goods on show: clothes models, weapon racks and cabinets.
    Display,
    /// Arms and defence: cannons and cannonballs, standards, targets.
    Military,
    /// A ship's or a dock's fittings: wheel, anchor, mast, gangplank,
    /// nets, rope, winches.
    Ship,
    /// Milling: hoppers and millstones.
    Mill,
    /// Farming: hay, troughs, coops and hutches, ploughs.
    Farm,
    /// Magic: symbols, crystal balls, portals.
    Magic,
    /// Learning: bookcases, study desks, scrolls, charts, telescopes.
    Books,
    /// Cooking: ranges, ovens, cooking pots, spits, hanging meat.
    Cooking,
    /// Washing: sinks, baths, washbasins, mangles.
    Washing,
    /// A bed or hammock.
    Bed,
    /// A chair, stool or bench.
    Seat,
    /// A table or desk.
    Table,
    /// Storage: crates, barrels, sacks, chests, drawers, cupboards, shelves.
    Storage,
    /// A fireplace or fire.
    Hearth,
    /// Light: candles, lamps, lanterns, torches.
    Light,
    /// Ornament: paintings, statues, vases, clocks, mounted heads, banners.
    Decoration,
    /// Plants and flowers.
    Plant,
    /// Graves and coffins.
    Grave,
    /// Confinement: cages, stocks, chains.
    Prison,
    /// A way up or down: ladders, stairs, trapdoors, climbing ropes.
    Stairs,
    /// Decay: corpses, skeletons and bones, broken furniture, webs, rubble.
    Remains,
    /// Part of the building: pillars, columns, plinths, supports, pipes,
    /// scaffolding.
    Fixture,
    /// Nature indoors or in a yard: trees, rocks, stalagmites, roots,
    /// fungus.
    Nature,
}

/// The keywords of each category, in the order names are tried.
const KEYWORDS: &[(Category, &[&str])] = &[
    (
        Category::Remains,
        &[
            "corpse", "skeleton", "skull", "skulls", "bones", "smashed", "broken", "web", "cocoon",
            "rubble", "bricks", "charred", "remains", "boarded",
        ],
    ),
    (
        Category::Bank,
        &[
            "bank booth",
            "bank table",
            "bank chest",
            "deposit box",
            "bank notice",
            "bank",
        ],
    ),
    (
        Category::Bar,
        &[
            "bar pumps",
            "barstool",
            "bar stool",
            "barrel tap",
            "keg",
            "bar",
        ],
    ),
    (
        Category::Smithing,
        &[
            "anvil",
            "furnace",
            "blacksmith's tools",
            "blacksmiths' tools",
            "forge",
            "bellows",
        ],
    ),
    (
        Category::Crafting,
        &[
            "spinning wheel",
            "potter's wheel",
            "pottery oven",
            "kiln",
            "loom",
            "tanning",
            "vat",
            "churn",
            "workbench",
            "work bench",
            "clothes equipment",
            "dye",
            "paint pots",
            "paintcans",
        ],
    ),
    (
        Category::Worship,
        &["altar", "pew", "organ", "font", "shrine", "pyre"],
    ),
    (Category::Throne, &["throne"]),
    (Category::Counter, &["counter", "stall"]),
    (
        Category::Display,
        &[
            "clothes model",
            "clothing shelves",
            "weapon rack",
            "sword cabinet",
            "bow cabinet",
            "axe cabinet",
            "display",
        ],
    ),
    (
        Category::Military,
        &[
            "cannon",
            "cannonballs",
            "multicannon",
            "standard",
            "flagpole",
            "target",
            "javelin",
            "armour",
            "dummy",
            "spear wall",
            "defence",
        ],
    ),
    (
        Category::Ship,
        &[
            "ship's wheel",
            "anchor",
            "mast",
            "gangplank",
            "figurehead",
            "jolly roger",
            "hull",
            "net",
            "rope",
            "winch",
            "crane",
        ],
    ),
    (Category::Mill, &["hopper", "millstones", "flour"]),
    (
        Category::Farm,
        &[
            "hay",
            "haystack",
            "trough",
            "coop",
            "hutch",
            "plough",
            "scarecrow",
            "eggs",
            "onion",
            "potato",
            "cabbage",
        ],
    ),
    (
        Category::Magic,
        &[
            "magical",
            "glyphs",
            "crystal ball",
            "portal",
            "pillar of light",
            "magic",
        ],
    ),
    (
        Category::Books,
        &[
            "bookcase",
            "bookshelf",
            "bookshelves",
            "study desk",
            "scrolls",
            "chart",
            "telescope",
            "globe",
        ],
    ),
    (
        Category::Cooking,
        &[
            "range",
            "oven",
            "cooking",
            "pot",
            "pots",
            "spit roast",
            "hanging meat",
            "cauldron",
            "kitchen",
            "stove",
        ],
    ),
    (Category::Washing, &["sink", "bath", "washbasin", "mangle"]),
    (Category::Bed, &["bed", "hammock", "bunk"]),
    (
        Category::Seat,
        &["chair", "stool", "bench", "chaise", "seat"],
    ),
    (Category::Table, &["table", "desk"]),
    (
        Category::Storage,
        &[
            "crate",
            "crates",
            "box",
            "boxes",
            "bucket",
            "broom",
            "barrel",
            "barrels",
            "sack",
            "sacks",
            "chest",
            "drawers",
            "cupboard",
            "cuboard",
            "cabinet",
            "shelves",
            "shelf",
            "wardrobe",
            "dresser",
            "rack",
            "basket",
            "locker",
            "urns",
            "hat stand",
        ],
    ),
    (Category::Hearth, &["fireplace", "fire"]),
    (
        Category::Light,
        &[
            "candles",
            "candle",
            "candlestick",
            "candelabrum",
            "lamp",
            "lantern",
            "torch",
            "chandelier",
        ],
    ),
    (
        Category::Decoration,
        &[
            "painting",
            "portrait",
            "landscape",
            "statue",
            "vase",
            "plaque",
            "mirror",
            "clock",
            "head",
            "stuffed",
            "banner",
            "tapestry",
            "aquarium",
            "bird cage",
            "screen",
            "rug",
            "piano",
            "fountain",
        ],
    ),
    (
        Category::Plant,
        &[
            "potted",
            "plant",
            "fern",
            "flower",
            "flowers",
            "flowerbed",
            "rosebush",
            "sunflower",
            "sunflowers",
            "daisies",
            "hedge",
            "bush",
            "vine",
        ],
    ),
    (
        Category::Grave,
        &["grave", "gravestone", "coffin", "tomb", "headstone"],
    ),
    (Category::Prison, &["cage", "stocks", "chains"]),
    (
        Category::Fixture,
        &[
            "pillar", "column", "plinth", "pedestal", "support", "scaffold", "pipe", "pipes",
        ],
    ),
    (
        Category::Nature,
        &[
            "tree",
            "rock",
            "rocks",
            "rockslide",
            "stalagmite",
            "stalagmites",
            "roots",
            "fungus",
            "fungi",
            "nettles",
            "log pile",
            "mushrooms",
        ],
    ),
    (
        Category::Stairs,
        &[
            "ladder",
            "staircase",
            "stairs",
            "stepladder",
            "trapdoor",
            "climbing rope",
        ],
    ),
];

/// The category of an object named `name`, if any. Words are compared
/// whole, ignoring case and punctuation other than apostrophes.
pub fn category(name: &str) -> Option<Category> {
    let words = normalise(name);
    KEYWORDS.iter().find_map(|(cat, keys)| {
        keys.iter()
            .any(|k| words.contains(&format!(" {k} ")))
            .then_some(*cat)
    })
}

/// `name` lower-cased, with anything but letters, digits and apostrophes
/// turned to spaces, padded with a space either side so keywords can be
/// matched as whole words.
fn normalise(name: &str) -> String {
    let mut s = String::with_capacity(name.len() + 2);
    s.push(' ');
    let mut space = true;
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() || c == '\'' {
            s.push(c);
            space = false;
        } else if !space {
            s.push(' ');
            space = true;
        }
    }
    if !space {
        s.push(' ');
    }
    s
}

/// What a minimap icon suggests about the place it marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "kind", content = "what")]
pub enum Hint {
    /// A shop selling this.
    Shop(&'static str),
    /// A bank.
    Bank,
    /// A pub or bar.
    Tavern,
    /// A smithy: a furnace or an anvil.
    Smithy,
    /// A place of worship: an altar.
    Worship,
    /// A cooking range.
    Cooking,
    /// A spinning wheel.
    Spinning,
    /// Pottery.
    Pottery,
    /// A water source.
    Water,
    /// A service or place of another kind.
    Service(&'static str),
}

/// What each minimap icon shows (the `mapfunction` sprites, by index, as a
/// location definition's opcode 60 names them), and what it suggests, read
/// off the sprites by eye. `None` where the picture doesn't say enough.
pub const ICONS: &[(u16, &str, Option<Hint>)] = &[
    (0, "a pot", Some(Hint::Shop("general"))),
    (1, "a sword", Some(Hint::Shop("swords"))),
    (2, "a flame", Some(Hint::Shop("magic"))),
    (3, "an axe", Some(Hint::Shop("axes"))),
    (4, "a helmet", Some(Hint::Shop("helmets"))),
    (5, "a dollar sign", Some(Hint::Bank)),
    (6, "a blue star", None),
    (7, "an amulet", Some(Hint::Shop("amulets"))),
    (8, "a pickaxe", Some(Hint::Service("mining"))),
    (9, "a furnace", Some(Hint::Smithy)),
    (10, "an anvil", Some(Hint::Smithy)),
    (
        11,
        "a training dummy on a frame",
        Some(Hint::Service("combat training")),
    ),
    (12, "an exclamation mark", None),
    (13, "a staff", Some(Hint::Shop("staffs"))),
    (14, "a platebody", Some(Hint::Shop("platebodies"))),
    (15, "platelegs", Some(Hint::Shop("platelegs"))),
    (16, "a scimitar", Some(Hint::Shop("scimitars"))),
    (17, "a hammer", None),
    (18, "a shield", Some(Hint::Shop("shields"))),
    (19, "an altar", Some(Hint::Worship)),
    (20, "herbs", Some(Hint::Shop("herbs"))),
    (21, "a ring", Some(Hint::Shop("jewellery"))),
    (22, "a gem", Some(Hint::Shop("gems"))),
    (23, "a chisel", Some(Hint::Shop("crafting"))),
    (24, "a candle", Some(Hint::Shop("candles"))),
    (25, "a fishing rod", Some(Hint::Shop("fishing"))),
    (26, "a fish", Some(Hint::Service("fishing"))),
    (27, "a green shirt", Some(Hint::Shop("clothes"))),
    (28, "a potion", Some(Hint::Shop("potions"))),
    (29, "a bolt of cloth", Some(Hint::Shop("cloth"))),
    (30, "a kebab", Some(Hint::Shop("kebabs"))),
    (31, "a beer mug", Some(Hint::Tavern)),
    (32, "a mace", Some(Hint::Shop("maces"))),
    (33, "a leather top", Some(Hint::Shop("leather"))),
    (34, "a tree", None),
    (35, "a spinning wheel", Some(Hint::Spinning)),
    (36, "a loaf", Some(Hint::Shop("food"))),
    (37, "a fork and knife", Some(Hint::Shop("cookery"))),
    (38, "a red star", None),
    (39, "a water drop", Some(Hint::Water)),
    (40, "a frying pan", Some(Hint::Cooking)),
    (41, "a skirt", Some(Hint::Shop("skirts"))),
    (42, "a clay bowl", Some(Hint::Pottery)),
    (43, "a yellow cross", None),
    (44, "a golden pickaxe", Some(Hint::Shop("mining"))),
    (45, "chainmail", Some(Hint::Shop("chainmail"))),
    (46, "a lump of ore", Some(Hint::Shop("silver"))),
    (47, "a pelt", Some(Hint::Shop("furs"))),
    (48, "a bowl of spice", Some(Hint::Shop("spices"))),
    (49, "a running figure", Some(Hint::Service("agility"))),
    (50, "an apple", Some(Hint::Shop("fruit"))),
    (51, "a skull", None),
    (52, "scissors", Some(Hint::Service("hairdresser"))),
    (53, "a spade", None),
    (54, "a face", Some(Hint::Service("makeover"))),
    (55, "a question mark", Some(Hint::Service("guide"))),
    (56, "an arrow", Some(Hint::Service("transport"))),
    (57, "a pink star", None),
    (58, "a clay pot", Some(Hint::Pottery)),
    (59, "a chequered flag", None),
    (60, "a barrel", None),
    (61, "a butter churn", Some(Hint::Service("dairy"))),
    (62, "a flask", None),
    (63, "a paw print", Some(Hint::Shop("pets"))),
    (64, "a purple orb", None),
    (65, "a hide", Some(Hint::Service("tannery"))),
    (66, "the letter i", Some(Hint::Service("information"))),
    (67, "a house", Some(Hint::Service("houses"))),
    (68, "a saw blade", Some(Hint::Service("sawmill"))),
    (69, "a hammer and stone", Some(Hint::Service("stonemason"))),
];

/// What minimap icon `index` shows and suggests, if it's a known icon.
pub fn icon(index: u16) -> Option<(&'static str, Option<Hint>)> {
    ICONS
        .iter()
        .find(|(i, _, _)| *i == index)
        .map(|&(_, shows, hint)| (shows, hint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_fall_in_the_first_category_that_fits() {
        assert_eq!(category("Bed"), Some(Category::Bed));
        assert_eq!(category("Bamboo Bed"), Some(Category::Bed));
        assert_eq!(category("Cooking shelves"), Some(Category::Cooking));
        assert_eq!(category("Shelves"), Some(Category::Storage));
        assert_eq!(category("Bar pumps"), Some(Category::Bar));
        assert_eq!(category("Barstool"), Some(Category::Bar));
        assert_eq!(category("Stool"), Some(Category::Seat));
        assert_eq!(category("Closed bank booth"), Some(Category::Bank));
        assert_eq!(category("Church pew"), Some(Category::Worship));
        assert_eq!(category("Blacksmith's tools"), Some(Category::Smithing));
        assert_eq!(category("Study desk"), Some(Category::Books));
        assert_eq!(category("Desk"), Some(Category::Table));
        assert_eq!(category("Pottery Oven"), Some(Category::Crafting));
        assert_eq!(category("Cooking range"), Some(Category::Cooking));
        assert_eq!(category("Smashed chair"), Some(Category::Remains));
        assert_eq!(category("Ship's wheel"), Some(Category::Ship));
        // Whole words only: a barrel isn't a bar, a potted plant isn't a pot.
        assert_eq!(category("Barrel"), Some(Category::Storage));
        assert_eq!(category("Potted plant"), Some(Category::Plant));
        assert_eq!(category("Something odd"), None);
    }

    #[test]
    fn icons_are_listed_once_in_order() {
        assert!(ICONS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(icon(5).map(|(_, h)| h), Some(Some(Hint::Bank)));
        assert_eq!(icon(31).and_then(|(_, h)| h), Some(Hint::Tavern));
        assert_eq!(icon(200), None);
    }
}
