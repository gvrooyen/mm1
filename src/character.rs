//! Native character state and the DOS roster compatibility boundary.

use serde::{Deserialize, Serialize};

pub const ROSTER_SLOTS: usize = 18;
const RECORD_SIZE: usize = 127;
const ROSTER_SIZE: usize = ROSTER_SLOTS * RECORD_SIZE + ROSTER_SLOTS;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct ValuePair {
    pub base: u8,
    pub current: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Character {
    pub name: String,
    pub sex: u8,
    pub initial_alignment: u8,
    pub current_alignment: u8,
    pub race: u8,
    pub class: u8,
    pub intellect: ValuePair,
    pub might: ValuePair,
    pub personality: ValuePair,
    pub endurance: ValuePair,
    pub speed: ValuePair,
    pub accuracy: ValuePair,
    pub luck: ValuePair,
    pub level: ValuePair,
    pub age: u8,
    pub age_counter: u8,
    pub experience: u32,
    pub current_spell_points: u16,
    pub maximum_spell_points: u16,
    pub spell_level: ValuePair,
    pub gems: u16,
    pub current_hp: u16,
    pub effective_max_hp: u16,
    pub base_max_hp: u16,
    pub gold: u32,
    pub armor_class: ValuePair,
    pub food: u8,
    pub condition: u8,
    pub equipped_items: [u8; 6],
    pub backpack_items: [u8; 6],
    pub equipped_charges: [u8; 6],
    pub backpack_charges: [u8; 6],
    pub resistances: [ValuePair; 8],
    pub physical_attribute: ValuePair,
    pub missile_attribute: ValuePair,
    pub trap_counter: u8,
    pub active_quest: u8,
    pub worthiness: u8,
    pub alignment_counter: u8,
    pub persistent_flags: [u8; 14],
    pub roster_index: u8,
}

/// A DOS roster slot. `metadata` is deliberately not native character state:
/// it records occupancy and possibly the inn/town used by the original game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RosterEntry {
    pub character: Character,
    pub metadata: u8,
}

pub fn decode_roster(data: &[u8]) -> Result<Vec<RosterEntry>, String> {
    if data.len() != ROSTER_SIZE {
        return Err(format!(
            "ROSTER.DTA is {} bytes; expected exactly {ROSTER_SIZE}",
            data.len()
        ));
    }

    (0..ROSTER_SLOTS)
        .map(|slot| {
            let start = slot * RECORD_SIZE;
            Ok(RosterEntry {
                character: decode_character(&data[start..start + RECORD_SIZE], slot)?,
                metadata: data[ROSTER_SLOTS * RECORD_SIZE + slot],
            })
        })
        .collect()
}

fn decode_character(record: &[u8], slot: usize) -> Result<Character, String> {
    let name_field = &record[..16];
    let nul = name_field
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| {
            format!("ROSTER.DTA slot {slot} name is not NUL-padded (maximum 15 characters)")
        })?;
    if name_field[nul..].iter().any(|byte| *byte != 0) {
        return Err(format!(
            "ROSTER.DTA slot {slot} has data after its name NUL"
        ));
    }
    if !name_field[..nul].is_ascii() {
        return Err(format!("ROSTER.DTA slot {slot} name is not ASCII"));
    }
    let name = String::from_utf8(name_field[..nul].to_vec())
        .map_err(|_| format!("ROSTER.DTA slot {slot} name is not valid text"))?;
    let pair = |offset| ValuePair {
        base: record[offset],
        current: record[offset + 1],
    };
    let u16_at = |offset| u16::from_le_bytes([record[offset], record[offset + 1]]);
    let array = |offset| record[offset..offset + 6].try_into().unwrap();

    Ok(Character {
        name,
        sex: record[0x10],
        initial_alignment: record[0x11],
        current_alignment: record[0x12],
        race: record[0x13],
        class: record[0x14],
        intellect: pair(0x15),
        might: pair(0x17),
        personality: pair(0x19),
        endurance: pair(0x1b),
        speed: pair(0x1d),
        accuracy: pair(0x1f),
        luck: pair(0x21),
        level: pair(0x23),
        age: record[0x25],
        age_counter: record[0x26],
        experience: u32::from_le_bytes(record[0x27..0x2b].try_into().unwrap()),
        current_spell_points: u16_at(0x2b),
        maximum_spell_points: u16_at(0x2d),
        spell_level: pair(0x2f),
        gems: u16_at(0x31),
        current_hp: u16_at(0x33),
        effective_max_hp: u16_at(0x35),
        base_max_hp: u16_at(0x37),
        gold: record[0x39] as u32 | (record[0x3a] as u32) << 8 | (record[0x3b] as u32) << 16,
        armor_class: pair(0x3c),
        food: record[0x3e],
        condition: record[0x3f],
        equipped_items: array(0x40),
        backpack_items: array(0x46),
        equipped_charges: array(0x4c),
        backpack_charges: array(0x52),
        resistances: std::array::from_fn(|index| pair(0x58 + index * 2)),
        physical_attribute: pair(0x68),
        missile_attribute: pair(0x6a),
        trap_counter: record[0x6c],
        active_quest: record[0x6d],
        worthiness: record[0x6e],
        alignment_counter: record[0x6f],
        persistent_flags: record[0x70..0x7e].try_into().unwrap(),
        roster_index: record[0x7e],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_roster_decodes_character_fields_and_metadata() {
        let roster = decode_roster(include_bytes!("../dos/ROSTER.DTA")).unwrap();
        assert_eq!(roster.len(), 18);
        assert_eq!(roster[0].character.name, "CRAG THE HACK");
        assert_eq!(roster[0].character.experience, 60);
        assert_eq!(roster[0].character.gold, 200);
        assert_eq!(roster[0].metadata, 1);
        assert_eq!(roster[6].metadata, 0);
        assert_eq!(roster[17].character.roster_index, 17);
    }

    #[test]
    fn malformed_roster_size_is_rejected() {
        assert!(
            decode_roster(&[0; ROSTER_SIZE - 1])
                .unwrap_err()
                .contains("2304")
        );
    }
}
