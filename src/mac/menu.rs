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
    Messenger,
    Pause,
    ContextWarnings,
    IdleWarnings,
    Config,
    ApplyTest,
    Apply,
    Exit,
}

impl Command {
    pub const ALL: [Command; 13] = [
        Command::CodexSummary,
        Command::ClaudeSummary,
        Command::Sound,
        Command::Visual,
        Command::Messenger,
        Command::Pause,
        Command::ContextWarnings,
        Command::IdleWarnings,
        Command::About,
        Command::Config,
        Command::ApplyTest,
        Command::Apply,
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
/// `messenger` is the state of the messenger switch, or `None` when no sink is configured (the
/// item is then not listed), as on the Windows tray.
pub fn items(
    sound: bool,
    visual: bool,
    messenger: Option<bool>,
    paused: bool,
    context: bool,
    idle: bool,
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
    let has_usage = codex.is_some() || claude.is_some();
    // The order and the separators of the Windows tray menu (`tray.rs`).
    let rest = [
        (Command::Sound, "Sound".to_owned(), sound),
        (Command::Visual, "Pop-up".to_owned(), visual),
        (
            Command::Messenger,
            "Messenger".to_owned(),
            messenger.unwrap_or(false),
        ),
        (Command::Pause, "Pause notifications".to_owned(), paused),
        (
            Command::ContextWarnings,
            "Context warnings".to_owned(),
            context,
        ),
        (Command::IdleWarnings, "Idle warnings".to_owned(), idle),
        (Command::About, version, false),
        (Command::Config, "Edit configuration".to_owned(), false),
        (Command::ApplyTest, "Apply + Test".to_owned(), false),
        (Command::Apply, "Apply".to_owned(), false),
        (Command::Exit, "Exit".to_owned(), false),
    ];
    usage
        .chain(rest)
        .filter(|(command, ..)| *command != Command::Messenger || messenger.is_some())
        .map(|(command, label, checked)| Item {
            command,
            label,
            checked,
            separator_before: match command {
                Command::Sound => has_usage,
                Command::ContextWarnings | Command::About | Command::Config => true,
                _ => false,
            },
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
            None,
            true,
            true,
            false,
            Some("Codex usage (unavailable)"),
            Some("Claude remain: 5h 80%"),
        );
        assert_eq!(list.len(), 12);
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
        assert!(item(&list, Command::ContextWarnings).checked);
        assert!(!item(&list, Command::IdleWarnings).checked);
        assert_eq!(
            item(&list, Command::ContextWarnings).label,
            "Context warnings"
        );
        assert_eq!(item(&list, Command::IdleWarnings).label, "Idle warnings");
        assert_eq!(item(&list, Command::Visual).label, "Pop-up");
        assert_eq!(item(&list, Command::Pause).label, "Pause notifications");
        assert_eq!(item(&list, Command::Config).label, "Edit configuration");
        assert_eq!(item(&list, Command::ApplyTest).label, "Apply + Test");
        assert_eq!(item(&list, Command::Apply).label, "Apply");
        assert_eq!(
            item(&list, Command::About).label,
            format!("aHerald v{}", crate::VERSION)
        );
        assert!(!item(&list, Command::About).checked);
    }

    #[test]
    fn usage_lines_appear_only_for_enabled_agents_claude_first() {
        let none = items(true, true, None, false, false, false, None, None);
        assert_eq!(none.len(), 10);
        let claude = items(true, true, None, false, false, false, None, Some("Claude"));
        assert!(!commands(&claude).contains(&Command::CodexSummary));
        let both = items(
            true,
            true,
            None,
            false,
            false,
            false,
            Some("Codex"),
            Some("Claude"),
        );
        assert_eq!(
            commands(&both)[..3],
            [
                Command::ClaudeSummary,
                Command::CodexSummary,
                Command::Sound
            ]
        );
    }

    /// The Windows tray's order (`tray.rs`): usage lines, the three output switches, the two
    /// warning switches, then the version and the configuration commands.
    #[test]
    fn the_menu_has_the_windows_order_and_separators() {
        for (codex, claude) in [
            (None, None),
            (Some("c"), None),
            (None, Some("a")),
            (Some("c"), Some("a")),
        ] {
            let list = items(true, true, None, false, false, false, codex, claude);
            let has_usage = codex.is_some() || claude.is_some();
            let separated: Vec<Command> = list
                .iter()
                .filter(|i| i.separator_before)
                .map(|i| i.command)
                .collect();
            let expected: Vec<Command> = has_usage
                .then_some(Command::Sound)
                .into_iter()
                .chain([Command::ContextWarnings, Command::About, Command::Config])
                .collect();
            assert_eq!(separated, expected);
            assert!(!list[0].separator_before);
            let tail: Vec<Command> = commands(&list)
                .into_iter()
                .skip_while(|c| *c != Command::Sound)
                .collect();
            assert_eq!(
                tail,
                [
                    Command::Sound,
                    Command::Visual,
                    Command::Pause,
                    Command::ContextWarnings,
                    Command::IdleWarnings,
                    Command::About,
                    Command::Config,
                    Command::ApplyTest,
                    Command::Apply,
                    Command::Exit
                ]
            );
        }
    }

    /// Windows order: Sound, Pop-up, Messenger, Pause. The item exists only when a sink runs.
    #[test]
    fn the_messenger_switch_follows_pop_up_and_only_when_configured() {
        let absent = items(true, true, None, false, false, false, None, None);
        assert!(!commands(&absent).contains(&Command::Messenger));
        for state in [false, true] {
            let list = items(true, true, Some(state), false, false, false, None, None);
            let order = commands(&list);
            let at = order.iter().position(|c| *c == Command::Messenger).unwrap();
            assert_eq!(order[at - 1], Command::Visual);
            assert_eq!(order[at + 1], Command::Pause);
            assert_eq!(item(&list, Command::Messenger).label, "Messenger");
            assert_eq!(item(&list, Command::Messenger).checked, state);
            assert!(!item(&list, Command::Messenger).separator_before);
        }
    }

    #[test]
    fn tags_round_trip_and_zero_is_no_command() {
        for command in Command::ALL {
            assert_eq!(Command::from_tag(command.tag()), Some(command));
        }
        assert_eq!(Command::from_tag(0), None);
        assert_eq!(Command::from_tag(-3), None);
        assert_eq!(Command::from_tag(Command::ALL.len() as isize + 1), None);
    }
}
