pub const PARAM_COUNT: usize = 8;
pub const GATE_OFF_DB: f32 = -96.0;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Clean = 0,
    Crunch = 1,
    Overdrive = 2,
    Distortion = 3,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Param {
    InputGain = 0,
    Drive,
    Bass,
    Mid,
    Treble,
    Presence,
    Master,
    Gate,
}

#[derive(Clone, Copy, Debug)]
pub struct Voicing {
    pub tight_hz: f32,
    pub drive_min_db: f32,
    pub drive_max_db: f32,
    pub bias: f32,
    pub hardness: f32,
    pub mid_hz: f32,
    pub level_db: f32,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Clean, Preset::Crunch, Preset::Overdrive, Preset::Distortion];

    pub fn from_index(i: u32) -> Option<Preset> {
        Self::ALL.get(i as usize).copied()
    }

    /// Knob values indexed by `Param as usize`.
    pub fn knobs(self) -> [f32; PARAM_COUNT] {
        match self {
            Preset::Clean => [0.5, 0.2, 0.5, 0.55, 0.6, 0.5, 0.6, -96.0],
            Preset::Crunch => [0.5, 0.5, 0.55, 0.6, 0.6, 0.55, 0.55, -70.0],
            Preset::Overdrive => [0.5, 0.6, 0.55, 0.65, 0.55, 0.5, 0.5, -65.0],
            Preset::Distortion => [0.5, 0.75, 0.65, 0.4, 0.6, 0.55, 0.45, -50.0],
        }
    }

    pub fn voicing(self) -> Voicing {
        let v = |tight_hz, drive_min_db, drive_max_db, bias, hardness, mid_hz, level_db| Voicing {
            tight_hz, drive_min_db, drive_max_db, bias, hardness, mid_hz, level_db,
        };
        match self {
            Preset::Clean => v(40.0, -12.0, 12.0, 0.05, 0.0, 600.0, 20.0),
            Preset::Crunch => v(80.0, 10.0, 34.0, 0.10, 0.5, 700.0, -3.0),
            Preset::Overdrive => v(120.0, 6.0, 36.0, 0.10, 0.3, 750.0, -2.0),
            Preset::Distortion => v(170.0, 18.0, 54.0, 0.08, 0.7, 650.0, -3.0),
        }
    }
}

impl Param {
    pub const ALL: [Param; PARAM_COUNT] = [
        Param::InputGain, Param::Drive, Param::Bass, Param::Mid,
        Param::Treble, Param::Presence, Param::Master, Param::Gate,
    ];

    pub fn from_index(i: u32) -> Option<Param> {
        Self::ALL.get(i as usize).copied()
    }

    pub fn range(self) -> (f32, f32) {
        match self {
            Param::Gate => (GATE_OFF_DB, -30.0),
            _ => (0.0, 1.0),
        }
    }

    /// Clamp into range; `None` for NaN/Inf.
    pub fn clamp(self, v: f32) -> Option<f32> {
        let (lo, hi) = self.range();
        v.is_finite().then(|| v.clamp(lo, hi))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_index_roundtrip() {
        for p in Preset::ALL {
            assert_eq!(Preset::from_index(p as u32), Some(p));
        }
        assert_eq!(Preset::from_index(4), None);
        for p in Param::ALL {
            assert_eq!(Param::from_index(p as u32), Some(p));
        }
        assert_eq!(Param::from_index(8), None);
    }

    #[test]
    fn clamp_limits() {
        assert_eq!(Param::Bass.clamp(1.5), Some(1.0));
        assert_eq!(Param::Bass.clamp(-1.0), Some(0.0));
        assert_eq!(Param::Gate.clamp(-200.0), Some(-96.0));
        assert_eq!(Param::Gate.clamp(0.0), Some(-30.0));
        assert_eq!(Param::Mid.clamp(f32::NAN), None);
        assert_eq!(Param::Mid.clamp(f32::INFINITY), None);
    }

    #[test]
    fn preset_tables_in_range() {
        for pr in Preset::ALL {
            let k = pr.knobs();
            for p in Param::ALL {
                let v = k[p as usize];
                assert_eq!(p.clamp(v), Some(v), "{pr:?} {p:?}");
            }
        }
    }
}
