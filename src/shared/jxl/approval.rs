use clap::{Arg, ArgAction, ArgMatches, Command};
use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::Path,
};

#[derive(Clone, Copy, Default)]
pub(crate) struct Policy {
    pub allow_loss: bool,
    pub dry_run: bool,
}

pub(crate) fn arguments(command: Command) -> Command {
    command
        .arg(
            Arg::new("lossless")
                .long("lossless")
                .help("Use lossless JXL encoding (exact pixels supplied to the encoder)")
                .conflicts_with("quality")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("allow-iptc-loss")
                .long("allow-iptc-loss")
                .help("Allow JXL output to omit standalone IPTC metadata without prompting")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Report planned outputs and IPTC approvals without writing or prompting")
                .action(ArgAction::SetTrue),
        )
}

impl Policy {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Self {
        Self {
            allow_loss: matches.get_flag("allow-iptc-loss"),
            dry_run: matches.get_flag("dry-run"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Answer {
    Yes,
    No,
    All,
}

fn answer(input: &str) -> Option<Answer> {
    match input.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Some(Answer::Yes),
        "" | "n" | "no" => Some(Answer::No),
        "a" | "all" | "yes for all" => Some(Answer::All),
        _ => None,
    }
}

pub(crate) struct Approval {
    policy: Policy,
    all: bool,
}

impl Approval {
    pub(crate) fn new(policy: Policy) -> Self {
        Self {
            policy,
            all: policy.allow_loss,
        }
    }

    fn decide(&mut self, answer: Answer) -> bool {
        if answer == Answer::All {
            self.all = true;
        }
        answer != Answer::No
    }

    // Called sequentially before starting progress bars or parallel conversions.
    pub(crate) fn check(
        &mut self,
        input: &Path,
        output: &Path,
        clear: bool,
    ) -> Result<bool, String> {
        if !super::is_jxl(output) || clear {
            return Ok(true);
        }
        let data =
            fs::read(input).map_err(|e| format!("Cannot inspect {}: {e}", input.display()))?;
        let profiles = crate::shared::metadata::profiles::extract(input, &data)?;
        if profiles.iptc.as_ref().is_none_or(Vec::is_empty) {
            return Ok(true);
        }
        if self.policy.dry_run {
            println!(
                "{}: JXL output would require approval to omit standalone IPTC{}",
                input.display(),
                if self.all { " (already allowed)" } else { "" }
            );
            return Ok(true);
        }
        if self.all {
            return Ok(true);
        }
        if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
            return Err(format!(
                "Skipping {}: JXL cannot preserve standalone IPTC; use --allow-iptc-loss or --clear-metadata",
                input.display()
            ));
        }
        loop {
            eprint!(
                "{}: JXL cannot preserve standalone IPTC. Continue? [y/N/a (yes for all)] ",
                input.display()
            );
            io::stderr().flush().map_err(|e| e.to_string())?;
            let mut input = String::new();
            if io::stdin()
                .read_line(&mut input)
                .map_err(|e| e.to_string())?
                == 0
            {
                return Ok(false);
            }
            match answer(&input) {
                Some(answer) => return Ok(self.decide(answer)),
                None => eprintln!("Enter yes, no, or a (yes for all)."),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_answers_default_to_no() {
        for s in ["", "n", "NO"] {
            assert_eq!(answer(s), Some(Answer::No));
        }
        assert_eq!(answer("yes"), Some(Answer::Yes));
        assert_eq!(answer("a"), Some(Answer::All));
        assert_eq!(answer("yes for all"), Some(Answer::All));
        assert_eq!(answer("invalid"), None);
    }
    #[test]
    fn only_yes_for_all_authorizes_later_files() {
        let mut approval = Approval::new(Policy::default());
        assert!(!approval.decide(Answer::No));
        assert!(!approval.all);
        assert!(approval.decide(Answer::Yes));
        assert!(!approval.all);
        assert!(approval.decide(Answer::All));
        assert!(approval.all);
        assert!(!Approval::new(Policy::default()).all);
    }
}
