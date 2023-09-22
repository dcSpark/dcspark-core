const EPOCH_LENGTH_IN_SECONDS: u64 = 432000;
const BYRON_SLOT_DURATION: u64 = 20;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Era {
    pub first_slot: u64,
    pub start_epoch: u64,
    pub known_time: u64,
    pub slot_length: u64,
    pub epoch_length_seconds: u64,
}

impl Era {
    pub const SHELLEY_MAINNET: Self = Self {
        first_slot: 4492800,
        start_epoch: 208,
        known_time: 1596059091,
        slot_length: 1,
        epoch_length_seconds: 432000,
    };

    pub const SHELLEY_TESTNET: Self = Self {
        first_slot: 1598400,
        start_epoch: 74,
        known_time: 1595967616,
        slot_length: 1,
        epoch_length_seconds: 432000,
    };

    pub const SHELLEY_PREPROD: Self = Self {
        first_slot: 86400,
        start_epoch: 4,
        known_time: 1655769600,
        slot_length: 1,
        epoch_length_seconds: 432000,
    };

    pub const SHELLEY_PREVIEW: Self = Self {
        first_slot: 0,
        start_epoch: 0,
        known_time: 1666656000,
        slot_length: 1,
        epoch_length_seconds: 86400,
    };

    pub const SHELLEY_SANCHO: Self = Self {
        first_slot: 0,
        start_epoch: 0,
        known_time: 1686789000,
        slot_length: 1,
        epoch_length_seconds: 86400,
    };

    pub const fn compute_timestamp(&self, slot: u64) -> u64 {
        self.known_time + (slot - self.first_slot) * self.slot_length
    }

    pub fn absolute_slot_to_epoch(&self, slot: u64) -> Option<u64> {
        slot.checked_sub(self.first_slot)
            .map(|slot_relative_to_era| {
                self.start_epoch + slot_relative_to_era / self.epoch_length_seconds
            })
    }

    pub fn absolute_slot_to_relative(&self, slot: u64) -> Option<u64> {
        slot.checked_sub(self.first_slot)
            .map(|slot_relative_to_era| slot_relative_to_era % self.epoch_length_seconds)
    }
}

pub const fn epoch_slot_to_absolute_byron(epoch: u64, epoch_slot: u64) -> u64 {
    let slots_per_epoch = EPOCH_LENGTH_IN_SECONDS / BYRON_SLOT_DURATION;
    epoch * slots_per_epoch + epoch_slot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_slot_to_epoch_mainnet() {
        let era = Era::SHELLEY_MAINNET;

        assert_eq!(None, era.absolute_slot_to_epoch(4492800 - 1));
        assert_eq!(Some(208), era.absolute_slot_to_epoch(4492800));
        assert_eq!(Some(208), era.absolute_slot_to_epoch(4492840));
        assert_eq!(
            Some(208),
            era.absolute_slot_to_epoch(era.first_slot + era.epoch_length_seconds - 1)
        );
        assert_eq!(
            Some(209),
            era.absolute_slot_to_epoch(era.first_slot + era.epoch_length_seconds)
        );

        let correct = 92595;
        let slot = epoch_slot_to_absolute_byron(4, 6195);
        assert_eq!(slot, correct);

        let epoch = era.absolute_slot_to_epoch(97507251).unwrap();
        assert_eq!(epoch, 423);

        let epoch = era.absolute_slot_to_epoch(99406785).unwrap();
        assert_eq!(epoch, 427);

        let epoch = era.absolute_slot_to_epoch(99100793).unwrap();
        assert_eq!(epoch, 426);

        let epoch = era.absolute_slot_to_epoch(99100852).unwrap();
        assert_eq!(epoch, 427);

        let relative_slot = era.absolute_slot_to_relative(99100852).unwrap();
        assert_eq!(relative_slot, 52);
    }

    #[test]
    fn absolute_slot_to_epoch_preprod() {
        let era = Era::SHELLEY_PREPROD;
        let epoch = era.absolute_slot_to_epoch(33389374).unwrap();
        assert_eq!(epoch, 81);
        let epoch = era.absolute_slot_to_epoch(33350429).unwrap();
        assert_eq!(epoch, 81);
        let timestamp = era.compute_timestamp(33350429);
        assert_eq!(timestamp, 1689033629);
        let epoch = era.absolute_slot_to_epoch(33346852).unwrap();
        assert_eq!(epoch, 80);
        let epoch = era.absolute_slot_to_epoch(33350398).unwrap();
        assert_eq!(epoch, 80);
        let epoch = era.absolute_slot_to_epoch(518340).unwrap();
        assert_eq!(epoch, 4);
    }

    #[test]
    fn absolute_slot_to_epoch_preview() {
        let era = Era::SHELLEY_PREVIEW;
        let epoch = era.absolute_slot_to_epoch(24317818).unwrap();
        assert_eq!(epoch, 281);
        let relative_slot = era.absolute_slot_to_relative(24317818).unwrap();
        assert_eq!(relative_slot, 39418);

        let epoch = era.absolute_slot_to_epoch(24278443).unwrap();
        assert_eq!(epoch, 281);
        let relative_slot = era.absolute_slot_to_relative(24278443).unwrap();
        assert_eq!(relative_slot, 43);

        let epoch = era.absolute_slot_to_epoch(24278356).unwrap();
        assert_eq!(epoch, 280);
        let relative_slot = era.absolute_slot_to_relative(24278356).unwrap();
        assert_eq!(relative_slot, 86356);
        let timestamp = era.compute_timestamp(24278356);
        assert_eq!(timestamp, 1690934356);

        let epoch = era.absolute_slot_to_epoch(100).unwrap();
        assert_eq!(epoch, 0);
        let relative_slot = era.absolute_slot_to_relative(100).unwrap();
        assert_eq!(relative_slot, 100);
    }
}
