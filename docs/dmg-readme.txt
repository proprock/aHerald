aHerald for macOS

1. Drag aHerald.app onto the Applications folder.
2. Open aHerald from Applications. This build is not signed with a Developer ID and is not
   notarized, so macOS blocks the first start: Control-click the app, choose Open, then Open.
   (Or run once in Terminal: xattr -dr com.apple.quarantine /Applications/aHerald.app)
3. The aHerald Setup window opens. Choose Codex and/or Claude Code, press Preview to see
   the changes, then Apply. Each agent still asks you to trust the hooks.

To remove the hooks later, open aHerald again, clear both choices and press Apply, then
drag aHerald.app to the Trash. Your configuration, logs and backups stay in
~/Library/Application Support/aHerald.
