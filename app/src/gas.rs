//! The gas blown through the nozzle with the beam (GLOSSARY: Gas): the
//! original's numbered list, the job's pick, and the gas a job is cut with
//! when it has none of its own (MECHANICS GS10–GS14).

/// One gas of the list, numbered the way the machine knows it (GS10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gas {
    Nitrogen,
    Air,
    Oxygen,
    Argon,
    Water,
}

impl Gas {
    /// The list, in the order of its numbers.
    pub const ALL: [Gas; 5] = [Gas::Nitrogen, Gas::Air, Gas::Oxygen, Gas::Argon, Gas::Water];

    /// GS10: the number the LC5 program writes (`;IDGAS=`, `<GasId>`).
    pub fn number(self) -> u32 {
        match self {
            Gas::Nitrogen => 1,
            Gas::Air => 2,
            Gas::Oxygen => 3,
            Gas::Argon => 4,
            Gas::Water => 5,
        }
    }

    /// GS10: the name the LC5 program writes (`;NOMEGAS=`, `<GasName>`).
    pub fn name(self) -> &'static str {
        match self {
            Gas::Nitrogen => "Nitrogen",
            Gas::Air => "Air",
            Gas::Oxygen => "Oxygen",
            Gas::Argon => "Argon",
            Gas::Water => "Water",
        }
    }

    /// "Oxygen (3)".
    pub fn label(self) -> String {
        format!("{} ({})", self.name(), self.number())
    }

    pub fn from_number(n: u32) -> Option<Gas> {
        Gas::ALL.into_iter().find(|g| g.number() == n)
    }

    /// GS14: a gas as a person or an agent gives it: its name in any case, or
    /// its number. Anything else is refused with the list.
    pub fn parse(text: &str) -> Result<Gas, String> {
        let t = text.trim();
        let by_number = t.parse::<f64>().ok().filter(|n| n.fract() == 0.0 && *n >= 0.0 && *n <= u32::MAX as f64).and_then(|n| Gas::from_number(n as u32));
        by_number
            .or_else(|| Gas::ALL.into_iter().find(|g| g.name().eq_ignore_ascii_case(t)))
            .ok_or_else(|| format!("gas '{t}' is not one of the gases: {}. Give its name or number.", list_words()))
    }

    /// The list's gas for a machine's saved gas number and name, when they
    /// are the list's pair exactly (GS12). A pair that isn't, such as 2 named
    /// "Oxygen", belongs to no gas of the list.
    pub fn listed(number: u32, name: &str) -> Option<Gas> {
        Gas::from_number(number).filter(|g| g.name() == name)
    }
}

/// The list in words: "1 Nitrogen, 2 Air, 3 Oxygen, 4 Argon, 5 Water".
pub fn list_words() -> String {
    Gas::ALL.iter().map(|g| format!("{} {}", g.number(), g.name())).collect::<Vec<_>>().join(", ")
}

/// A gas number from a machine file or technology table, for people:
/// "Oxygen (3)" for a gas of the list, else "gas 7".
pub fn number_label(n: i64) -> String {
    u32::try_from(n).ok().and_then(Gas::from_number).map_or_else(|| format!("gas {n}"), Gas::label)
}

/// The gas a job is cut with, as its programs name it (GS11).
#[derive(Clone, Debug, PartialEq)]
pub struct JobGas {
    pub number: u32,
    pub name: String,
    /// The job's own gas; false = the machine's, for a job saved before jobs
    /// carried a gas.
    pub own: bool,
}

impl JobGas {
    /// GS11: the job's own gas; a job without one is cut with the machine's
    /// gas, its number and name as saved in Settings → Machines, even when
    /// they aren't the list's pair.
    pub fn of(job: Option<Gas>, machine_number: u32, machine_name: &str) -> JobGas {
        match job {
            Some(g) => JobGas { number: g.number(), name: g.name().to_string(), own: true },
            None => JobGas { number: machine_number, name: machine_name.to_string(), own: false },
        }
    }

    /// "Oxygen (3)".
    pub fn label(&self) -> String {
        format!("{} ({})", self.name, self.number)
    }

    /// The list's gas this is, when its number and name are the list's pair.
    pub fn listed(&self) -> Option<Gas> {
        Gas::listed(self.number, &self.name)
    }
}

/// GS13: the gas number the machine file's material × thickness section is
/// picked by (MTL4): the job's own. A job without one gives none (0), so
/// any section fits it, as before jobs carried a gas.
pub fn section_gas(job: Option<Gas>) -> i32 {
    job.map_or(0, |g| g.number() as i32)
}

/// GS13: the gas number the technology table's record is picked by (TT9):
/// the job's own; for a job without one, the machine's, as before.
pub fn record_gas(job: Option<Gas>, machine_number: u32) -> u32 {
    job.map_or(machine_number, Gas::number)
}

/// A job file keeps the gas as its number (GS10). Reading is forgiving: a
/// name is taken too, and a number or word that isn't a gas of the list
/// reads as no gas of the job's own (MAC-69), so the job still opens.
pub mod number {
    use super::Gas;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(g: &Option<Gas>, s: S) -> Result<S::Ok, S::Error> {
        match g {
            Some(g) => s.serialize_u32(g.number()),
            None => s.serialize_none(),
        }
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(f64),
        Text(String),
        Other(serde::de::IgnoredAny),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Gas>, D::Error> {
        Ok(match Option::<Raw>::deserialize(d)? {
            Some(Raw::Number(n)) => Gas::parse(&n.to_string()).ok(),
            Some(Raw::Text(t)) => Gas::parse(&t).ok(),
            Some(Raw::Other(_)) | None => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_the_originals_numbers_and_names() {
        let list: Vec<(u32, &str)> = Gas::ALL.iter().map(|g| (g.number(), g.name())).collect();
        assert_eq!(list, [(1, "Nitrogen"), (2, "Air"), (3, "Oxygen"), (4, "Argon"), (5, "Water")]);
        assert_eq!(Gas::Oxygen.label(), "Oxygen (3)");
        assert_eq!(list_words(), "1 Nitrogen, 2 Air, 3 Oxygen, 4 Argon, 5 Water");
        assert_eq!((number_label(3), number_label(7), number_label(-1)), ("Oxygen (3)".into(), "gas 7".into(), "gas -1".into()));
    }

    #[test]
    fn a_gas_is_given_by_its_name_in_any_case_or_its_number() {
        for t in ["oxygen", "OXYGEN", " Oxygen ", "3", "3.0"] {
            assert_eq!(Gas::parse(t), Ok(Gas::Oxygen), "{t}");
        }
        for t in ["0", "6", "-1", "2.5", "helium", "", "oxy", "1e400"] {
            let e = Gas::parse(t).unwrap_err();
            assert!(e.contains("1 Nitrogen, 2 Air, 3 Oxygen, 4 Argon, 5 Water"), "{t}: {e}");
        }
    }

    #[test]
    fn a_machine_gas_is_on_the_list_only_as_the_lists_exact_pair() {
        assert_eq!(Gas::listed(1, "Nitrogen"), Some(Gas::Nitrogen));
        assert_eq!(Gas::listed(3, "Oxygen"), Some(Gas::Oxygen));
        for (n, name) in [(2, "Oxygen"), (1, "NITROGEN"), (0, "Nitrogen"), (6, "Water"), (1, "")] {
            assert_eq!(Gas::listed(n, name), None, "{n} {name}");
        }
    }

    #[test]
    fn a_job_without_a_gas_is_cut_with_the_machines_as_saved() {
        let j = JobGas::of(None, 2, "Oxygen");
        assert_eq!((j.number, j.name.as_str(), j.own, j.listed()), (2, "Oxygen", false, None));
        let j = JobGas::of(Some(Gas::Oxygen), 2, "Oxygen");
        assert_eq!((j.label(), j.own, j.listed()), ("Oxygen (3)".to_string(), true, Some(Gas::Oxygen)));
        // The section and record keys: none and the machine's without a gas of the job's own.
        assert_eq!((section_gas(None), record_gas(None, 2)), (0, 2));
        assert_eq!((section_gas(Some(Gas::Oxygen)), record_gas(Some(Gas::Oxygen), 2)), (3, 3));
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct Holder {
        #[serde(default, skip_serializing_if = "Option::is_none", with = "number")]
        gas: Option<Gas>,
    }

    #[test]
    fn a_job_file_keeps_the_gas_as_its_number_and_reads_old_and_odd_files() {
        assert_eq!(serde_json::to_string(&Holder { gas: Some(Gas::Oxygen) }).unwrap(), r#"{"gas":3}"#);
        assert_eq!(serde_json::to_string(&Holder { gas: None }).unwrap(), "{}");
        let read = |t: &str| serde_json::from_str::<Holder>(t).unwrap().gas;
        assert_eq!(read("{}"), None);
        assert_eq!(read(r#"{"gas":3}"#), Some(Gas::Oxygen));
        assert_eq!(read(r#"{"gas":"oxygen"}"#), Some(Gas::Oxygen));
        for odd in [r#"{"gas":null}"#, r#"{"gas":0}"#, r#"{"gas":6}"#, r#"{"gas":"helium"}"#, r#"{"gas":[3]}"#, r#"{"gas":true}"#] {
            assert_eq!(read(odd), None, "{odd}");
        }
    }
}
