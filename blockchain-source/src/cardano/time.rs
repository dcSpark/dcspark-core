const EPOCH_LENGTH_IN_SECONDS: u64 = 43200;
const BYRON_SLOT_DURATION: u64 = 20;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Era {
    pub first_slot: u64,
    pub start_epoch: u64,
    pub known_time: u64,
    pub slot_length: u64,
}

impl Era {
    pub const SHELLEY_MAINNET: Self = Self {
        first_slot: 4492800,
        start_epoch: 208,
        known_time: 1596059091,
        slot_length: 1,
    };

    pub const SHELLEY_PREPROD: Self = Self {
        first_slot: 86400,
        start_epoch: 0,
        known_time: 1655769600,
        slot_length: 1,
    };

    pub const SHELLEY_PREVIEW: Self = Self {
        first_slot: 0,
        start_epoch: 0,
        known_time: 1660003200,
        slot_length: 1,
    };

    pub const fn compute_timestamp(&self, slot: u64) -> u64 {
        self.known_time + (slot - self.first_slot) * self.slot_length
    }

    pub fn absolute_slot_to_epoch(&self, slot: u64) -> Option<u64> {
        slot.checked_sub(self.first_slot)
            .map(|slot_relative_to_era| {
                self.start_epoch + slot_relative_to_era / EPOCH_LENGTH_IN_SECONDS
            })
    }
}

pub const fn epoch_slot_to_absolute(epoch: u64, epoch_slot: u64) -> u64 {
    let slots_per_epoch = EPOCH_LENGTH_IN_SECONDS / BYRON_SLOT_DURATION;
    epoch * slots_per_epoch + epoch_slot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_slot_to_epoch() {
        let era = Era::SHELLEY_MAINNET;

        assert_eq!(None, era.absolute_slot_to_epoch(4492800 - 1));
        assert_eq!(Some(208), era.absolute_slot_to_epoch(4492800));
        assert_eq!(Some(208), era.absolute_slot_to_epoch(4492840));
        assert_eq!(
            Some(208),
            era.absolute_slot_to_epoch(era.first_slot + EPOCH_LENGTH_IN_SECONDS - 1)
        );
        assert_eq!(
            Some(209),
            era.absolute_slot_to_epoch(era.first_slot + EPOCH_LENGTH_IN_SECONDS)
        );
    }
}
