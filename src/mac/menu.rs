//! What the status item's menu lists. Pure: the AppKit side only turns these entries into
//! `NSMenuItem`s, so order, labels and check marks are the same contract as the Windows
//! tray menu (`tray.rs`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    CodexSummary,
    ClaudeSummary,
    About,
    Sound,
    Visual,
    Pause,
    Config,
    Restart,
    Exit,
}

impl Command {
    pub const ALL: [Command; 9] = [
        Command::CodexSummary,
        Command::ClaudeSummary,
        Command::About,
        Command::Sound,
        Command::Visual,
        Command::Pause,
        Command::Config,
        Command::Restart,
        Command::Exit,
    ];

    /// The menu item tag AppKit hands back when the item is chosen.
    pub fn tag(self) -> isize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0) as isize + 1
    }

    pub fn from_tag(tag: isize) -> Option<Self> {
        usize::try_from(tag - 1)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub command: Command,
    pub label: String,
    pub checked: bool,
    /// A separator line is drawn above this item.
    pub separator_before: bool,
}

/// `codex` and `claude` are the usage lines of the enabled agents; each opens its own details.
pub fn items(
    sound: bool,
    visual: bool,
    paused: bool,
    codex: Option<&str>,
    claude: Option<&str>,
) -> Vec<Item> {
    let version = format!("aHerald v{}", crate::VERSION);
    // Alphabetical by agent name, as on Windows.
    let usage = [
        (Command::ClaudeSummary, claude),
        (Command::CodexSummary, codex),
    ]
    .into_iter()
    .filter_map(|(command, text)| text.map(|text| (command, text.to_owned(), false)));
    let rest = [
        (Command::About, version, false),
        (Command::Sound, "Sound".to_owned(), sound),
        (Command::Visual, "Pop-up".to_owned(), visual),
        (Command::Pause, "Pause notifications".to_owned(), paused),
        (Command::Config, "Edit configuration".to_owned(), false),
        (Command::Restart, "Restart".to_owned(), false),
        (Command::Exit, "Exit".to_owned(), false),
    ];
    usage
        .chain(rest)
        .map(|(command, label, checked)| Item {
            command,
            label,
            checked,
            separator_before: matches!(command, Command::Sound | Command::Config),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands(items: &[Item]) -> Vec<Command> {
        items.iter().map(|i| i.command).collect()
    }
    fn item(items: &[Item], command: Command) -> &Item {
        items.iter().find(|i| i.command == command).unwrap()
    }

    #[test]
    fn the_menu_lists_every_action_with_the_current_check_marks() {
        let list = items(
            false,
            true,
            true,
            Some("Codex usage (unavailable)"),
            Some("Claude remain: 5h 80%"),
        );
        assert_eq!(list.len(), 9);
        assert!(!item(&list, Command::Sound).checked);
        assert!(item(&list, Command::Visual).checked);
        assert!(item(&list, Command::Pause).checked);
        assert!(
            item(&list, Command::ClaudeSummary)
                .label
                .contains("Claude remain")
        );
        assert!(
            item(&list, Command::CodexSummary)
                .label
                .contains("unavailable")
        );
        assert_eq!(item(&list, Command::Visual).label, "Pop-up");
        assert_eq!(item(&list, Command::Pause).label, "Pause notifications");
        assert_eq!(item(&list, Command::Config).label, "Edit configuration");
        assert_eq!(item(&list, Command::Restart).label, "Restart");
        assert_eq!(
            item(&list, Command::About).label,
            format!("aHerald v{}", crate::VERSION)
        );
        assert!(!item(&list, Command::About).checked);
    }

    #[test]
    fn usage_lines_appear_only_for_enabled_agents_claude_first() {
        let none = items(true, true, false, None, None);
        assert_eq!(none.len(), 7);
        let claude = items(true, true, false, None, Some("Claude"));
        assert!(!commands(&claude).contains(&Command::CodexSummary));
        let both = items(true, true, false, Some("Codex"), Some("Claude"));
        assert_eq!(
            commands(&both)[..3],
            [
                Command::ClaudeSummary,
                Command::CodexSummary,
                Command::About
            ]
        );
    }

    #[test]
    fn separators_follow_the_version_and_pause_without_a_leading_separator() {
        for (codex, claude) in [
            (None, None),
            (Some("c"), None),
            (None, Some("a")),
            (Some("c"), Some("a")),
        ] {
            let list = items(true, true, false, codex, claude);
            let separated: Vec<Command> = list
                .iter()
                .filter(|i| i.separator_before)
                .map(|i| i.command)
                .collect();
            assert_eq!(separated, [Command::Sound, Command::Config]);
            assert!(!list[0].separator_before);
            let tail: Vec<Command> = commands(&list)
                .into_iter()
                .skip_while(|c| *c != Command::About)
                .collect();
            assert_eq!(
                tail,
                [
                    Command::About,
                    Command::Sound,
                    Command::Visual,
                    Command::Pause,
                    Command::Config,
                    Command::Restart,
                    Command::Exit
                ]
            );
        }
    }

    #[test]
    fn tags_round_trip_and_zero_is_no_command() {
        for command in Command::ALL {
            assert_eq!(Command::from_tag(command.tag()), Some(command));
        }
        assert_eq!(Command::from_tag(0), None);
        assert_eq!(Command::from_tag(-3), None);
        assert_eq!(Command::from_tag(10), None);
    }
}
