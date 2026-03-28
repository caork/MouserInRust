#![allow(dead_code)]
// app_catalog.rs — Static catalog of well-known applications.
//
// Ported from core/app_catalog.py.  The catalog contains curated entries for
// high-value applications on Windows and macOS.  Each entry carries:
//
//   exe          — primary identifier used in profile matching
//                  (Windows: basename like "chrome.exe",
//                   macOS:   bundle ID like "com.google.Chrome")
//   display_name — human-readable name shown in the UI
//   icon_hint    — optional legacy icon filename (may be empty)
//
// Lookup helpers:
//   find_app_by_exe(exe)        — exact case-insensitive match on `exe`
//   find_apps_by_name(query)    — case-insensitive substring match on display_name

// ---------------------------------------------------------------------------
// Data type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    /// Primary identifier (executable name or bundle ID).
    pub exe: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Optional legacy icon filename; `None` means no icon hint.
    pub icon_hint: Option<&'static str>,
}

// ---------------------------------------------------------------------------
// Helper macro for concise catalog entries
// ---------------------------------------------------------------------------

macro_rules! app {
    ($exe:expr, $name:expr) => {
        AppEntry { exe: $exe, display_name: $name, icon_hint: None }
    };
    ($exe:expr, $name:expr, $icon:expr) => {
        AppEntry { exe: $exe, display_name: $name, icon_hint: Some($icon) }
    };
}

// ---------------------------------------------------------------------------
// Static catalog
// ---------------------------------------------------------------------------

pub static APP_CATALOG: &[AppEntry] = &[
    // -----------------------------------------------------------------------
    // Windows apps
    // -----------------------------------------------------------------------

    // Browsers
    app!("msedge.exe",   "Microsoft Edge"),
    app!("chrome.exe",   "Google Chrome",      "chrom.png"),
    app!("firefox.exe",  "Firefox"),
    app!("iexplore.exe", "Internet Explorer"),
    app!("opera.exe",    "Opera"),
    app!("brave.exe",    "Brave"),
    app!("vivaldi.exe",  "Vivaldi"),

    // Editors / IDEs
    app!("Code.exe",           "Visual Studio Code",  "VSCODE.png"),
    app!("Cursor.exe",         "Cursor"),
    app!("devenv.exe",         "Visual Studio"),
    app!("idea64.exe",         "IntelliJ IDEA"),
    app!("webstorm64.exe",     "WebStorm"),
    app!("pycharm64.exe",      "PyCharm"),
    app!("clion64.exe",        "CLion"),
    app!("rider64.exe",        "Rider"),
    app!("goland64.exe",       "GoLand"),
    app!("sublime_text.exe",   "Sublime Text"),
    app!("atom.exe",           "Atom"),
    app!("notepad++.exe",      "Notepad++"),
    app!("notepad.exe",        "Notepad"),
    app!("wordpad.exe",        "WordPad"),

    // Office
    app!("WINWORD.EXE",    "Microsoft Word"),
    app!("EXCEL.EXE",      "Microsoft Excel"),
    app!("POWERPNT.EXE",   "Microsoft PowerPoint"),
    app!("OUTLOOK.EXE",    "Microsoft Outlook"),
    app!("ONENOTE.EXE",    "Microsoft OneNote"),
    app!("MSPUB.EXE",      "Microsoft Publisher"),
    app!("MSACCESS.EXE",   "Microsoft Access"),

    // Creative / Adobe
    app!("Adobe Premiere Pro.exe", "Adobe Premiere Pro"),
    app!("AfterFX.exe",            "Adobe After Effects"),
    app!("Photoshop.exe",          "Adobe Photoshop"),
    app!("Illustrator.exe",        "Adobe Illustrator"),
    app!("InDesign.exe",           "Adobe InDesign"),
    app!("Lightroom.exe",          "Adobe Lightroom"),
    app!("Acrobat.exe",            "Adobe Acrobat"),

    // Communication
    app!("slack.exe",   "Slack"),
    app!("Discord.exe", "Discord"),
    app!("teams.exe",   "Microsoft Teams"),
    app!("zoom.exe",    "Zoom"),
    app!("Skype.exe",   "Skype"),
    app!("signal.exe",  "Signal"),

    // Media
    app!("Spotify.exe",                 "Spotify"),
    app!("vlc.exe",                     "VLC Media Player",  "VLC.png"),
    app!("Microsoft.Media.Player.exe",  "Windows Media Player", "media.webp"),
    app!("wmplayer.exe",                "Windows Media Player"),
    app!("mpv.exe",                     "mpv"),
    app!("mpc-hc64.exe",               "MPC-HC"),
    app!("iTunes.exe",                  "iTunes"),

    // Terminal / Shell
    app!("WindowsTerminal.exe", "Windows Terminal"),
    app!("wt.exe",              "Windows Terminal"),
    app!("cmd.exe",             "Command Prompt"),
    app!("powershell.exe",      "Windows PowerShell"),
    app!("pwsh.exe",            "PowerShell"),
    app!("wsl.exe",             "WSL"),

    // File management / Shell
    app!("explorer.exe", "File Explorer"),

    // Development tools
    app!("GitHubDesktop.exe", "GitHub Desktop"),
    app!("gitkraken.exe",     "GitKraken"),
    app!("sourcetree.exe",    "Sourcetree"),
    app!("docker.exe",        "Docker Desktop"),
    app!("Postman.exe",       "Postman"),
    app!("insomnia.exe",      "Insomnia"),
    app!("dbeaver.exe",       "DBeaver"),
    app!("TablePlus.exe",     "TablePlus"),
    app!("HeidiSQL.exe",      "HeidiSQL"),

    // 3-D / CAD / Design
    app!("blender.exe",    "Blender"),
    app!("AutoCAD.exe",    "AutoCAD"),
    app!("SOLIDWORKS.exe", "SOLIDWORKS"),
    app!("FreeCAD.exe",    "FreeCAD"),

    // Games / launchers
    app!("steam.exe",   "Steam"),
    app!("EpicGamesLauncher.exe", "Epic Games Launcher"),
    app!("Battle.net.exe",        "Battle.net"),

    // Utilities
    app!("7zFM.exe",      "7-Zip"),
    app!("winrar.exe",    "WinRAR"),
    app!("calc.exe",      "Calculator"),
    app!("mspaint.exe",   "Paint"),
    app!("taskmgr.exe",   "Task Manager"),

    // -----------------------------------------------------------------------
    // macOS apps  (bundle IDs)
    // -----------------------------------------------------------------------

    // Browsers
    app!("com.apple.Safari",              "Safari"),
    app!("com.google.Chrome",             "Google Chrome",  "chrom.png"),
    app!("org.mozilla.firefox",           "Firefox"),
    app!("com.microsoft.edgemac",         "Microsoft Edge"),
    app!("com.brave.Browser",             "Brave"),
    app!("com.operasoftware.Opera",       "Opera"),
    app!("com.vivaldi.Vivaldi",           "Vivaldi"),
    app!("com.apple.WebKit.WebContent",   "WebKit WebContent"),

    // Editors / IDEs
    app!("com.microsoft.VSCode",         "Visual Studio Code",  "VSCODE.png"),
    app!("com.todesktop.230313mzl4w4u92", "Cursor"),
    app!("com.jetbrains.intellij",        "IntelliJ IDEA"),
    app!("com.jetbrains.webstorm",        "WebStorm"),
    app!("com.jetbrains.pycharm",         "PyCharm"),
    app!("com.sublimetext.4",             "Sublime Text"),
    app!("com.github.atom",               "Atom"),
    app!("com.apple.TextEdit",            "TextEdit"),
    app!("com.barebones.bbedit",          "BBEdit"),

    // Office / Productivity
    app!("com.microsoft.Word",        "Microsoft Word"),
    app!("com.microsoft.Excel",       "Microsoft Excel"),
    app!("com.microsoft.Powerpoint",  "Microsoft PowerPoint"),
    app!("com.microsoft.Outlook",     "Microsoft Outlook"),
    app!("com.microsoft.onenote.mac", "Microsoft OneNote"),
    app!("com.apple.iWork.Pages",     "Pages"),
    app!("com.apple.iWork.Numbers",   "Numbers"),
    app!("com.apple.iWork.Keynote",   "Keynote"),
    app!("com.apple.Notes",           "Notes"),
    app!("com.culturedcode.ThingsMac","Things 3"),
    app!("com.omnigroup.OmniFocus3",  "OmniFocus"),
    app!("md.obsidian",               "Obsidian"),
    app!("com.notion.id",             "Notion"),

    // Creative / Adobe
    app!("com.adobe.AdobePremierePro",  "Adobe Premiere Pro"),
    app!("com.adobe.AfterEffects",      "Adobe After Effects"),
    app!("com.adobe.Photoshop",         "Adobe Photoshop"),
    app!("com.adobe.illustrator",       "Adobe Illustrator"),
    app!("com.adobe.InDesign",          "Adobe InDesign"),
    app!("com.adobe.Lightroom",         "Adobe Lightroom"),
    app!("com.adobe.Reader",            "Adobe Acrobat Reader"),
    app!("com.adobe.Acrobat.Pro",       "Adobe Acrobat Pro"),
    app!("com.bohemiancoding.sketch3",  "Sketch"),
    app!("com.figma.Desktop",           "Figma"),
    app!("com.affinity.designer2",      "Affinity Designer"),
    app!("com.affinity.photo2",         "Affinity Photo"),

    // Communication
    app!("com.tinyspeck.slackmacgap",     "Slack"),
    app!("com.hnc.Discord",               "Discord"),
    app!("com.microsoft.teams2",          "Microsoft Teams"),
    app!("us.zoom.xos",                   "Zoom"),
    app!("com.skype.skype",               "Skype"),
    app!("org.whispersystems.signal-desktop", "Signal"),
    app!("ru.keepcoder.Telegram",         "Telegram"),
    app!("com.apple.MobileSMS",           "Messages"),
    app!("com.apple.FaceTime",            "FaceTime"),
    app!("com.apple.mail",                "Mail"),

    // Media
    app!("com.spotify.client",        "Spotify"),
    app!("org.videolan.vlc",          "VLC Media Player",  "VLC.png"),
    app!("com.apple.Music",           "Music"),
    app!("com.apple.TV",              "TV"),
    app!("com.apple.QuickTimePlayerX","QuickTime Player"),
    app!("io.mpv",                    "mpv"),
    app!("org.gimp.gimp-2.10",        "GIMP"),

    // Terminal / Shell
    app!("com.apple.Terminal",              "Terminal"),
    app!("com.googlecode.iterm2",           "iTerm2"),
    app!("com.github.wez.wezterm",          "WezTerm"),
    app!("co.zeit.hyper",                   "Hyper"),
    app!("com.mitchellh.ghostty",           "Ghostty"),

    // File management
    app!("com.apple.finder",          "Finder"),
    app!("com.binarynights.ForkLift3","ForkLift"),
    app!("com.panic.Transmit5",       "Transmit"),

    // Development tools
    app!("com.github.GitHubClient",   "GitHub Desktop"),
    app!("com.axosoft.gitkraken",     "GitKraken"),
    app!("com.atlassian.sourcetree",  "Sourcetree"),
    app!("com.docker.docker",         "Docker Desktop"),
    app!("com.postmanlabs.mac",       "Postman"),
    app!("com.insomnia.app",          "Insomnia"),
    app!("com.tinyapp.TablePlus",     "TablePlus"),
    app!("com.dbeaver.product",       "DBeaver"),

    // 3-D / Design
    app!("org.blender",               "Blender"),
    app!("com.autodesk.AutoCAD",      "AutoCAD"),

    // Games / launchers
    app!("com.valvesoftware.steam",   "Steam"),
    app!("com.epicgames.EpicGamesLauncher", "Epic Games Launcher"),

    // Utilities
    app!("com.apple.systempreferences",    "System Settings"),
    app!("com.apple.ActivityMonitor",      "Activity Monitor"),
    app!("com.apple.calculator",           "Calculator"),
    app!("com.apple.Preview",              "Preview"),
    app!("com.apple.ScreenSaver.Engine",   "Screen Saver"),
    app!("com.apple.screencaptureui",      "Screenshot"),
    app!("org.7-zip.7-Zip",               "7-Zip"),
    app!("com.aone.keka",                  "Keka"),
    app!("com.macpaw.CleanMyMac4",         "CleanMyMac"),
    app!("com.apple.Automator",            "Automator"),
    app!("com.apple.ScriptEditor2",        "Script Editor"),
];

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

/// Find an `AppEntry` by its `exe` field (case-insensitive exact match).
pub fn find_app_by_exe(exe: &str) -> Option<&'static AppEntry> {
    let lower = exe.to_lowercase();
    APP_CATALOG.iter().find(|e| e.exe.to_lowercase() == lower)
}

/// Find all `AppEntry` values whose `display_name` contains `query`
/// (case-insensitive substring match).
pub fn find_apps_by_name(query: &str) -> Vec<&'static AppEntry> {
    let lower = query.to_lowercase();
    APP_CATALOG
        .iter()
        .filter(|e| e.display_name.to_lowercase().contains(&lower))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_non_empty() {
        assert!(!APP_CATALOG.is_empty(), "APP_CATALOG must not be empty");
    }

    #[test]
    fn test_find_chrome_windows() {
        let entry = find_app_by_exe("chrome.exe");
        assert!(entry.is_some(), "chrome.exe must be in the catalog");
        let entry = entry.unwrap();
        assert_eq!(entry.display_name, "Google Chrome");
        assert_eq!(entry.icon_hint, Some("chrom.png"));
    }

    #[test]
    fn test_find_chrome_macos() {
        let entry = find_app_by_exe("com.google.Chrome");
        assert!(entry.is_some(), "com.google.Chrome must be in the catalog");
        assert_eq!(entry.unwrap().display_name, "Google Chrome");
    }

    #[test]
    fn test_find_app_by_exe_case_insensitive() {
        assert!(find_app_by_exe("CHROME.EXE").is_some());
        assert!(find_app_by_exe("Chrome.Exe").is_some());
    }

    #[test]
    fn test_find_app_by_exe_missing() {
        assert!(find_app_by_exe("this_does_not_exist.exe").is_none());
    }

    #[test]
    fn test_find_apps_by_name_substring() {
        let results = find_apps_by_name("Adobe");
        assert!(!results.is_empty(), "should find at least one Adobe app");
        for e in &results {
            assert!(
                e.display_name.to_lowercase().contains("adobe"),
                "unexpected entry: {}",
                e.display_name
            );
        }
    }

    #[test]
    fn test_find_apps_by_name_empty_query() {
        // Empty query matches everything
        let results = find_apps_by_name("");
        assert_eq!(results.len(), APP_CATALOG.len());
    }

    #[test]
    fn test_no_duplicate_exes() {
        let mut seen = std::collections::HashSet::new();
        for entry in APP_CATALOG {
            let lower = entry.exe.to_lowercase();
            assert!(
                seen.insert(lower.clone()),
                "Duplicate exe in APP_CATALOG: {}",
                entry.exe
            );
        }
    }

    #[test]
    fn test_vscode_windows_and_macos_both_present() {
        assert!(find_app_by_exe("Code.exe").is_some());
        assert!(find_app_by_exe("com.microsoft.VSCode").is_some());
    }

    #[test]
    fn test_finder_present() {
        assert!(find_app_by_exe("com.apple.finder").is_some());
    }

    #[test]
    fn test_vlc_has_icon_hint() {
        let entry = find_app_by_exe("vlc.exe").unwrap();
        assert_eq!(entry.icon_hint, Some("VLC.png"));
    }
}
