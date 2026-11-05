//! Interactive setup choices shared by the Linux console and the macOS window.
//!
//! Pure data: it turns the current receipt and settings into defaults, and the
//! user's final choices into arguments for [`crate::setup::run_with_options`].
use crate::{agent::Agent, bridge_setup::Bridge, setup::UsageOptions};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub codex: bool,
    pub claude: bool,
    pub codex_usage: bool,
    pub claude_usage: bool,
    pub codex_context: bool,
    pub claude_context: bool,
    pub codex_idle: bool,
    pub claude_idle: bool,
    /// The bridge files of the harnesses that have no hooks of their own.
    pub opencode: bool,
    pub pi: bool,
    pub goose: bool,
}

impl Selection {
    /// Defaults from the saved receipt and the `--setup-settings` report.
    pub fn initial(previous: &[Agent], settings: &str) -> Self {
        let on = |key: &str| settings.lines().any(|line| line == format!("{key}=true"));
        let mut selection = Self {
            codex: previous.contains(&Agent::Codex),
            claude: previous.contains(&Agent::Claude),
            codex_usage: on("codex_usage"),
            claude_usage: on("claude_usage"),
            codex_context: on("codex_context"),
            claude_context: on("claude_context"),
            codex_idle: on("codex_idle"),
            claude_idle: on("claude_idle"),
            ..Default::default()
        };
        for bridge in Bridge::ALL {
            selection.set_bridge(bridge, on(&format!("{}_bridge", bridge.key())));
        }
        selection
    }

    /// Whether `bridge` is selected. The one place that maps a bridge to its field, so a new
    /// `Bridge` variant fails to compile here until it has one.
    pub fn bridge(&self, bridge: Bridge) -> bool {
        match bridge {
            Bridge::OpenCode => self.opencode,
            Bridge::Pi => self.pi,
            Bridge::Goose => self.goose,
        }
    }

    pub fn set_bridge(&mut self, bridge: Bridge, selected: bool) {
        match bridge {
            Bridge::OpenCode => self.opencode = selected,
            Bridge::Pi => self.pi = selected,
            Bridge::Goose => self.goose = selected,
        }
    }

    /// Selected bridge files, in the order of the setup pages.
    pub fn bridges(&self) -> Vec<Bridge> {
        Bridge::ALL
            .into_iter()
            .filter(|bridge| self.bridge(*bridge))
            .collect()
    }

    /// Selected harnesses, Codex first.
    pub fn harnesses(&self) -> Vec<Agent> {
        [(self.codex, Agent::Codex), (self.claude, Agent::Claude)]
            .into_iter()
            .filter_map(|(selected, agent)| selected.then_some(agent))
            .collect()
    }

    /// Choosing no harness and no bridge removes everything setup saved.
    pub fn remove(&self) -> bool {
        !self.codex && !self.claude && self.bridges().is_empty()
    }

    /// Usage is always set; a warning applies to a selected agent only, so an
    /// unselected agent keeps its settings.
    pub fn options(&self) -> UsageOptions {
        UsageOptions {
            codex: Some(self.codex_usage),
            claude: Some(self.claude_usage),
            codex_context: self.codex.then_some(self.codex_context),
            claude_context: self.claude.then_some(self.claude_context),
            codex_idle: self.codex.then_some(self.codex_idle),
            claude_idle: self.claude.then_some(self.claude_idle),
            // A setup page knows the bridges, so its choice replaces the whole set.
            bridges: Some(self.bridges()),
            ..Default::default()
        }
    }
}

/// A double-click on an installed app bundle starts the program without arguments; that
/// opens the setup window instead of a usage error. Hooks and every command always pass
/// arguments, and a bare executable keeps reporting the missing operation.
pub fn launch_opens_setup(args: &[String], executable: &std::path::Path) -> bool {
    args.is_empty() && crate::paths::bundle_resources(executable).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const BUNDLE: &str = "/Applications/aHerald.app/Contents/MacOS/aherald";

    #[test]
    fn a_bundle_started_without_arguments_opens_the_setup_window() {
        assert!(launch_opens_setup(&[], Path::new(BUNDLE)));
    }

    #[test]
    fn arguments_or_a_bare_executable_keep_the_normal_behavior() {
        assert!(!launch_opens_setup(
            &["--hook".to_owned()],
            Path::new(BUNDLE)
        ));
        assert!(!launch_opens_setup(
            &[],
            Path::new("/usr/local/bin/aherald")
        ));
        assert!(!launch_opens_setup(
            &[],
            Path::new("/Applications/aHerald.app/aherald")
        ));
    }

    const OFF: &str = "codex_usage=false\nclaude_usage=false\ncodex_context=false\nclaude_context=false\ncodex_idle=false\nclaude_idle=false\n";

    #[test]
    fn first_setup_starts_with_nothing_selected() {
        let selection = Selection::initial(&[], OFF);
        assert_eq!(selection, Selection::default());
        assert!(selection.harnesses().is_empty());
        assert!(selection.remove());
    }

    #[test]
    fn upgrade_keeps_saved_harnesses_and_settings() {
        let settings = "codex_usage=true\nclaude_usage=false\ncodex_context=false\nclaude_context=true\ncodex_idle=true\nclaude_idle=false\n";
        let selection = Selection::initial(&[Agent::Claude], settings);
        assert!(!selection.codex && selection.claude);
        assert!(selection.codex_usage && !selection.claude_usage);
        assert!(selection.claude_context && !selection.codex_context);
        assert!(selection.codex_idle && !selection.claude_idle);
    }

    #[test]
    fn unknown_or_missing_settings_keys_default_to_off() {
        let selection = Selection::initial(&[], "codex_usage=maybe\nother=true\n");
        assert_eq!(selection, Selection::default());
    }

    #[test]
    fn harnesses_are_ordered_and_removal_follows_selection() {
        let both = Selection {
            codex: true,
            claude: true,
            ..Default::default()
        };
        assert_eq!(both.harnesses(), vec![Agent::Codex, Agent::Claude]);
        assert!(!both.remove());
        let claude = Selection {
            claude: true,
            ..Default::default()
        };
        assert_eq!(claude.harnesses(), vec![Agent::Claude]);
    }

    #[test]
    fn options_set_usage_and_limit_warnings_to_selected_agents() {
        let selection = Selection {
            codex: false,
            claude: true,
            codex_usage: true,
            claude_usage: false,
            codex_context: true,
            claude_context: true,
            codex_idle: true,
            claude_idle: false,
            ..Default::default()
        };
        let options = selection.options();
        assert_eq!(options.codex, Some(true));
        assert_eq!(options.claude, Some(false));
        assert_eq!(options.codex_context, None);
        assert_eq!(options.codex_idle, None);
        assert_eq!(options.claude_context, Some(true));
        assert_eq!(options.claude_idle, Some(false));
        assert_eq!(options.file, None);
        assert_eq!(options.executable, None);
        // A setup page names the bridges, so it replaces the set even when it is empty.
        assert_eq!(options.bridges, Some(Vec::new()));
    }

    #[test]
    fn bridges_come_from_the_settings_report_and_keep_a_setup_alive_without_hooks() {
        let report = format!("{OFF}opencode_bridge=true\npi_bridge=false\ngoose_bridge=true\n");
        let selection = Selection::initial(&[], &report);
        assert!(selection.opencode && !selection.pi && selection.goose);
        assert_eq!(selection.bridges(), vec![Bridge::OpenCode, Bridge::Goose]);
        assert!(!selection.remove(), "a bridge alone is not a removal");
        assert_eq!(
            selection.options().bridges,
            Some(vec![Bridge::OpenCode, Bridge::Goose])
        );
        let none = Selection::initial(&[], OFF);
        assert!(none.bridges().is_empty() && none.remove());
    }

    #[test]
    fn every_bridge_can_be_selected_alone_and_read_back_from_the_report() {
        for bridge in Bridge::ALL {
            let mut selection = Selection::default();
            assert!(!selection.bridge(bridge));
            selection.set_bridge(bridge, true);
            assert_eq!(selection.bridges(), vec![bridge], "{}", bridge.key());
            selection.set_bridge(bridge, false);
            assert!(selection.bridges().is_empty());

            // The settings report names a bridge `<key>_bridge`.
            let report = format!("{OFF}{}_bridge=true\n", bridge.key());
            assert_eq!(Selection::initial(&[], &report).bridges(), vec![bridge]);
        }
        let everything: String = Bridge::ALL
            .iter()
            .map(|bridge| format!("{}_bridge=true\n", bridge.key()))
            .collect();
        assert_eq!(
            Selection::initial(&[], &everything).bridges(),
            Bridge::ALL.to_vec()
        );
    }
}
