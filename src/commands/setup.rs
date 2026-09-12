use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::atomic_file;
use crate::cli::{SetupArgs, SetupMode};
use crate::shell::Shell;
use crate::util;

pub(crate) const BOOTSTRAP_START: &str = "# >>> recall setup bootstrap >>>";
pub(crate) const BOOTSTRAP_END: &str = "# <<< recall setup bootstrap <<<";
pub(crate) const INTEGRATION_START: &str = "# >>> recall setup integration >>>";
pub(crate) const INTEGRATION_END: &str = "# <<< recall setup integration <<<";
const INTEGRATION_NO_EOL_START: &str = "# >>> recall setup integration-no-eol >>>";
const INTEGRATION_NO_EOL_END: &str = "# <<< recall setup integration-no-eol <<<";
const LEGACY_START: &str = "# >>> recall installer >>>";
const LEGACY_END: &str = "# <<< recall installer <<<";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagedSetup {
    Auto,
    Hooks,
    LegacyAuto,
    LegacyHooks,
    UnmanagedHooks,
    None,
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProfileInspection {
    pub(crate) setup: ManagedSetup,
    pub(crate) init_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

struct ProfileText {
    text: String,
    encoding: TextEncoding,
    newline: &'static str,
}

pub fn run(args: SetupArgs) -> Result<()> {
    let result = apply(args.shell.as_deref(), args.profile, args.mode, args.remove)?;

    if args.remove {
        println!(
            "shell setup: {} ({})",
            result.profile.display(),
            if result.changed {
                "removed"
            } else {
                "not present"
            }
        );
        return Ok(());
    }

    println!(
        "shell setup: {} ({}; {})",
        result.profile.display(),
        match args.mode {
            SetupMode::Auto => "automatic output capture",
            SetupMode::Hooks => "hooks only",
        },
        if result.changed {
            "updated"
        } else {
            "already current"
        }
    );
    if args.mode == SetupMode::Auto {
        println!(
            "output capture starts automatically in new interactive shells; use --mode hooks to opt out"
        );
    }
    Ok(())
}

pub(crate) struct SetupResult {
    pub(crate) profile: PathBuf,
    pub(crate) changed: bool,
}

pub(crate) fn apply(
    shell_name: Option<&str>,
    profile: Option<PathBuf>,
    mode: SetupMode,
    remove: bool,
) -> Result<SetupResult> {
    let shell = resolve_shell(shell_name)?;
    let profile = profile
        .or_else(|| shell.rc_path())
        .ok_or_else(|| anyhow!("could not determine the startup file for {}", shell.name))?;
    let changed = update_profile(&profile, shell, mode, remove)?;
    Ok(SetupResult { profile, changed })
}

pub(crate) fn inspect_shell(shell_name: &str) -> Result<(PathBuf, ProfileInspection)> {
    let shell = resolve_shell(Some(shell_name))?;
    let profile = shell
        .rc_path()
        .ok_or_else(|| anyhow!("could not determine the startup file for {}", shell.name))?;
    let inspection = inspect_profile(&profile)?;
    Ok((profile, inspection))
}

pub(crate) fn inspect_profile(path: &Path) -> Result<ProfileInspection> {
    let path = resolve_profile_target(path)?;
    let profile = read_profile(&path)?;
    Ok(inspect_text(&profile.text))
}

fn inspect_text(text: &str) -> ProfileInspection {
    let init_count = active_init_count(text);
    if remove_managed_blocks(text).is_err() {
        return ProfileInspection {
            setup: ManagedSetup::Invalid,
            init_count,
        };
    }

    let setup = if let Some(block) = managed_block(text, BOOTSTRAP_START, BOOTSTRAP_END) {
        if block.contains("recall shell") {
            ManagedSetup::Auto
        } else {
            ManagedSetup::Hooks
        }
    } else if let Some(block) = managed_block(text, LEGACY_START, LEGACY_END) {
        if block.contains("recall shell") {
            ManagedSetup::LegacyAuto
        } else {
            ManagedSetup::LegacyHooks
        }
    } else if text.contains(INTEGRATION_START) || init_count > 0 {
        ManagedSetup::UnmanagedHooks
    } else {
        ManagedSetup::None
    };
    ProfileInspection { setup, init_count }
}

fn managed_block<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let start = text.find(start)? + start.len();
    let end = text[start..].find(end)? + start;
    Some(&text[start..end])
}

fn resolve_shell(name: Option<&str>) -> Result<&'static Shell> {
    let detected;
    let name = match name {
        Some(name) => name,
        None => {
            detected = util::login_shell();
            &detected
        }
    };
    Shell::from_command(name)
        .filter(|shell| shell.init_script().is_some())
        .ok_or_else(|| anyhow!("unsupported shell: {name}"))
}

fn update_profile(path: &Path, shell: &Shell, mode: SetupMode, remove: bool) -> Result<bool> {
    let write_path = resolve_profile_target(path)?;
    let existing = read_profile(&write_path)?;
    let cleaned = remove_managed_blocks(&existing.text)?;
    let rendered = if remove {
        cleaned
    } else {
        let binary_dir = std::env::current_exe()
            .context("locating the recall executable")?
            .parent()
            .ok_or_else(|| anyhow!("the recall executable has no parent directory"))?
            .to_path_buf();
        render_profile(&cleaned, shell.name, &binary_dir, mode)
    };

    if rendered == existing.text {
        return Ok(false);
    }
    write_profile(&write_path, &existing, &rendered)?;
    Ok(true)
}

fn resolve_profile_target(path: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)
            .with_context(|| format!("resolving startup file symlink {}", path.display())),
        Ok(_) => Ok(path.to_path_buf()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(err) => Err(err).with_context(|| format!("inspecting {}", path.display())),
    }
}

fn read_profile(path: &Path) -> Result<ProfileText> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProfileText {
                text: String::new(),
                encoding: TextEncoding::Utf8,
                newline: if cfg!(windows) { "\r\n" } else { "\n" },
            });
        }
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };

    let (text, encoding) = decode_text(&bytes).with_context(|| {
        format!(
            "decoding {} (expected UTF-8 or BOM-marked UTF-16)",
            path.display()
        )
    })?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    Ok(ProfileText {
        text: text.replace("\r\n", "\n"),
        encoding,
        newline,
    })
}

fn decode_text(bytes: &[u8]) -> Result<(String, TextEncoding)> {
    if let Some(bytes) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return Ok((String::from_utf8(bytes.to_vec())?, TextEncoding::Utf8Bom));
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return Ok((decode_utf16(bytes, true)?, TextEncoding::Utf16Le));
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return Ok((decode_utf16(bytes, false)?, TextEncoding::Utf16Be));
    }
    Ok((String::from_utf8(bytes.to_vec())?, TextEncoding::Utf8))
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String> {
    if !bytes.len().is_multiple_of(2) {
        bail!("UTF-16 input has an odd byte length");
    }
    let (pairs, _) = bytes.as_chunks::<2>();
    let units = pairs.iter().map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    String::from_utf16(&units.collect::<Vec<_>>()).map_err(Into::into)
}

fn encode_text(text: &str, encoding: TextEncoding, newline: &str) -> Vec<u8> {
    let text = if newline == "\n" {
        text.to_string()
    } else {
        text.replace('\n', newline)
    };
    match encoding {
        TextEncoding::Utf8 => text.into_bytes(),
        TextEncoding::Utf8Bom => [vec![0xef, 0xbb, 0xbf], text.into_bytes()].concat(),
        TextEncoding::Utf16Le => {
            let mut bytes = vec![0xff, 0xfe];
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            bytes
        }
        TextEncoding::Utf16Be => {
            let mut bytes = vec![0xfe, 0xff];
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
            bytes
        }
    }
}

fn remove_managed_blocks(text: &str) -> Result<String> {
    let mut output = String::new();
    let mut expected_end: Option<&str> = None;

    for line in text.split_inclusive('\n') {
        let marker = line.trim_end_matches(['\r', '\n']).trim();
        if let Some(end) = expected_end {
            if marker == end {
                expected_end = None;
            } else if is_start_marker(marker).is_some() || is_end_marker(marker) {
                bail!("malformed recall setup markers");
            }
            continue;
        }
        if let Some(end) = is_start_marker(marker) {
            if marker == INTEGRATION_NO_EOL_START && output.ends_with('\n') {
                output.pop();
            }
            expected_end = Some(end);
        } else if is_end_marker(marker) {
            bail!("malformed recall setup markers");
        } else {
            output.push_str(line);
        }
    }
    if expected_end.is_some() {
        bail!("malformed recall setup markers");
    }
    Ok(output)
}

fn is_start_marker(line: &str) -> Option<&'static str> {
    match line {
        BOOTSTRAP_START => Some(BOOTSTRAP_END),
        INTEGRATION_START => Some(INTEGRATION_END),
        INTEGRATION_NO_EOL_START => Some(INTEGRATION_NO_EOL_END),
        LEGACY_START => Some(LEGACY_END),
        _ => None,
    }
}

fn is_end_marker(line: &str) -> bool {
    matches!(
        line,
        BOOTSTRAP_END | INTEGRATION_END | INTEGRATION_NO_EOL_END | LEGACY_END
    )
}

fn render_profile(text: &str, shell: &str, binary_dir: &Path, mode: SetupMode) -> String {
    let mut output = String::new();
    output.push_str(&render_bootstrap(shell, binary_dir, mode));
    output.push_str(text);
    if active_init_count(text) == 0 {
        let preserve_no_eol = !text.is_empty() && !text.ends_with('\n');
        if preserve_no_eol {
            output.push('\n');
        }
        output.push_str(&render_integration(shell, preserve_no_eol));
    }
    output
}

fn render_bootstrap(shell: &str, binary_dir: &Path, mode: SetupMode) -> String {
    let path = binary_dir.to_string_lossy();
    match shell {
        "zsh" | "bash" => {
            let path = quote_posix(&path);
            let interactive = if shell == "zsh" {
                "[[ -o interactive ]]"
            } else {
                "[[ $- == *i* ]]"
            };
            let mut block = format!(
                "{BOOTSTRAP_START}\ncase \":$PATH:\" in\n  *\":{path}:\"*) ;;\n  *) export PATH={path}:\"$PATH\" ;;\nesac\n"
            );
            if mode == SetupMode::Auto {
                block.push_str(&format!(
                    "if {interactive} && [[ -t 0 && -t 1 && -z ${{RECALL_PROXY_ACTIVE:-}} && -z ${{RECALL_AUTO_LAUNCH:-}} && ${{RECALL_PROXY:-1}} != 0 ]]; then\n  export RECALL_AUTO_LAUNCH=1\n  exec recall shell\nfi\n"
                ));
            }
            block.push_str(BOOTSTRAP_END);
            block.push('\n');
            block
        }
        "fish" => {
            let path = quote_fish(&path);
            let mut block = format!(
                "{BOOTSTRAP_START}\ncontains -- {path} $PATH; or set -gx PATH {path} $PATH\n"
            );
            if mode == SetupMode::Auto {
                block.push_str(
                    "if status is-interactive; and isatty stdin; and isatty stdout; and not set -q RECALL_PROXY_ACTIVE; and not set -q RECALL_AUTO_LAUNCH\n    if not set -q RECALL_PROXY; or test \"$RECALL_PROXY\" != 0\n        set -gx RECALL_AUTO_LAUNCH 1\n        exec recall shell\n    end\nend\n",
                );
            }
            block.push_str(BOOTSTRAP_END);
            block.push('\n');
            block
        }
        "pwsh" | "powershell" => {
            let path = quote_powershell(&path);
            let mut block = format!(
                "{BOOTSTRAP_START}\nif (($env:Path -split ';') -notcontains {path}) {{ $env:Path = {path} + ';' + $env:Path }}\n"
            );
            if mode == SetupMode::Auto {
                block.push_str(
                    "if (-not [Console]::IsInputRedirected -and -not [Console]::IsOutputRedirected -and -not $env:RECALL_PROXY_ACTIVE -and -not $env:RECALL_AUTO_LAUNCH -and $env:RECALL_PROXY -ne '0') {\n    $env:RECALL_AUTO_LAUNCH = '1'\n    & recall shell\n    $recallExitCode = $LASTEXITCODE\n    exit $recallExitCode\n}\n",
                );
            }
            block.push_str(BOOTSTRAP_END);
            block.push('\n');
            block
        }
        _ => unreachable!("setup only accepts integrated shells"),
    }
}

fn render_integration(shell: &str, preserve_no_eol: bool) -> String {
    let command = match shell {
        "zsh" | "bash" => format!("eval \"$(command recall init {shell})\""),
        "fish" => "command recall init fish | source".to_string(),
        "pwsh" | "powershell" => {
            format!("recall init {shell} | Out-String | Invoke-Expression")
        }
        _ => unreachable!("setup only accepts integrated shells"),
    };
    let (start, end) = if preserve_no_eol {
        (INTEGRATION_NO_EOL_START, INTEGRATION_NO_EOL_END)
    } else {
        (INTEGRATION_START, INTEGRATION_END)
    };
    format!("{start}\n{command}\n{end}\n")
}

fn active_init_count(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let line = line.trim_start();
            !line.starts_with('#')
                && line.contains("recall init ")
                && (line.contains("eval")
                    || line.contains("source")
                    || line.contains("Invoke-Expression"))
        })
        .count()
}

fn quote_posix(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn quote_fish(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn quote_powershell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn write_profile(path: &Path, original: &ProfileText, text: &str) -> Result<()> {
    let bytes = encode_text(text, original.encoding, original.newline);
    atomic_file::replace(path, &bytes, "setup")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_profile(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "recall-setup-test-{}-{}-{name}",
            std::process::id(),
            ulid::Ulid::generate()
        ));
        fs::create_dir_all(&dir).unwrap();
        (dir.join("profile"), dir)
    }

    #[test]
    fn renders_fast_bootstrap_before_user_config_and_hooks_after_it() {
        let rendered = render_profile(
            "echo user-config\n",
            "zsh",
            Path::new("/opt/recall/bin"),
            SetupMode::Auto,
        );
        assert!(rendered.starts_with(BOOTSTRAP_START));
        assert!(rendered.contains("[[ -t 0 && -t 1"));
        assert!(rendered.contains("RECALL_PROXY_ACTIVE"));
        assert!(rendered.contains("RECALL_AUTO_LAUNCH"));
        assert!(rendered.contains("${RECALL_PROXY:-1} != 0"));
        assert!(
            rendered.find("echo user-config").unwrap() < rendered.find("recall init zsh").unwrap()
        );
    }

    #[test]
    fn hooks_mode_omits_proxy_launch() {
        let rendered = render_profile("", "bash", Path::new("/bin"), SetupMode::Hooks);
        assert!(!rendered.contains("exec recall shell"));
        assert!(rendered.contains("recall init bash"));
    }

    #[test]
    fn renders_shell_specific_launch_and_integration() {
        let fish = render_profile("", "fish", Path::new("/bin"), SetupMode::Auto);
        assert!(fish.contains("isatty stdin"));
        assert!(fish.contains("exec recall shell"));
        assert!(fish.contains("recall init fish | source"));

        let pwsh = render_profile("", "pwsh", Path::new("C:\\Recall"), SetupMode::Auto);
        assert!(pwsh.contains("[Console]::IsInputRedirected"));
        assert!(pwsh.contains("$recallExitCode = $LASTEXITCODE"));
        assert!(pwsh.contains("exit $recallExitCode"));
        assert!(pwsh.contains("recall init pwsh | Out-String | Invoke-Expression"));
    }

    #[test]
    fn keeps_an_existing_unmanaged_integration() {
        let original = "# custom\neval \"$(recall init zsh)\"\n";
        let rendered = render_profile(original, "zsh", Path::new("/bin"), SetupMode::Auto);
        assert_eq!(active_init_count(&rendered), 1);
        assert!(!rendered.contains(INTEGRATION_START));
    }

    #[test]
    fn does_not_treat_mentioned_init_commands_as_integration() {
        let original = "echo 'run recall init zsh later'\n";
        let rendered = render_profile(original, "zsh", Path::new("/bin"), SetupMode::Auto);
        assert_eq!(active_init_count(&rendered), 1);
        assert!(rendered.contains(INTEGRATION_START));
    }

    #[test]
    fn removes_legacy_and_current_managed_blocks() {
        let input = format!(
            "{LEGACY_START}\nold\n{LEGACY_END}\nkeep\n{BOOTSTRAP_START}\nnew\n{BOOTSTRAP_END}\n"
        );
        assert_eq!(remove_managed_blocks(&input).unwrap(), "keep\n");
    }

    #[test]
    fn inspects_managed_and_unmanaged_setup() {
        let auto = render_profile("", "zsh", Path::new("/bin"), SetupMode::Auto);
        assert_eq!(
            inspect_text(&auto),
            ProfileInspection {
                setup: ManagedSetup::Auto,
                init_count: 1,
            }
        );

        let hooks = render_profile("", "fish", Path::new("/bin"), SetupMode::Hooks);
        assert_eq!(inspect_text(&hooks).setup, ManagedSetup::Hooks);

        let unmanaged = "eval \"$(recall init bash)\"\n";
        assert_eq!(inspect_text(unmanaged).setup, ManagedSetup::UnmanagedHooks);
        assert_eq!(inspect_text(unmanaged).init_count, 1);

        let invalid = format!("{BOOTSTRAP_START}\n");
        assert_eq!(inspect_text(&invalid).setup, ManagedSetup::Invalid);
    }

    #[test]
    fn rejects_malformed_markers() {
        assert!(remove_managed_blocks(&format!("{BOOTSTRAP_START}\nmissing\n")).is_err());
        assert!(remove_managed_blocks(&format!("{INTEGRATION_END}\n")).is_err());
    }

    #[test]
    fn round_trips_supported_encodings() {
        for encoding in [
            TextEncoding::Utf8,
            TextEncoding::Utf8Bom,
            TextEncoding::Utf16Le,
            TextEncoding::Utf16Be,
        ] {
            let bytes = encode_text("hello 世界\n", encoding, "\r\n");
            let (decoded, actual) = decode_text(&bytes).unwrap();
            assert_eq!(actual, encoding);
            assert_eq!(decoded, "hello 世界\r\n");
        }
    }

    #[test]
    fn profile_update_is_idempotent_and_removable() {
        let (path, dir) = temp_profile("idempotent");
        fs::write(&path, "echo user\n").unwrap();
        let shell = Shell::from_name("zsh").unwrap();

        assert!(update_profile(&path, shell, SetupMode::Auto, false).unwrap());
        let once = fs::read(&path).unwrap();
        assert!(!update_profile(&path, shell, SetupMode::Auto, false).unwrap());
        assert_eq!(fs::read(&path).unwrap(), once);

        assert!(update_profile(&path, shell, SetupMode::Auto, true).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "echo user\n");
        assert!(!update_profile(&path, shell, SetupMode::Auto, true).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn shared_apply_api_updates_an_explicit_profile() {
        let (path, dir) = temp_profile("shared-api");
        fs::write(&path, "echo user\n").unwrap();

        let result = apply(Some("zsh"), Some(path.clone()), SetupMode::Hooks, false).unwrap();

        assert_eq!(result.profile, path);
        assert!(result.changed);
        assert_eq!(
            inspect_profile(&result.profile).unwrap().setup,
            ManagedSetup::Hooks
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn preserves_a_missing_final_newline() {
        let (path, dir) = temp_profile("no-final-newline");
        fs::write(&path, "echo user").unwrap();
        let shell = Shell::from_name("bash").unwrap();

        update_profile(&path, shell, SetupMode::Auto, false).unwrap();
        let once = fs::read(&path).unwrap();
        assert!(!update_profile(&path, shell, SetupMode::Auto, false).unwrap());
        assert_eq!(fs::read(&path).unwrap(), once);

        update_profile(&path, shell, SetupMode::Auto, true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"echo user");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn preserves_utf16_and_crlf() {
        let (path, dir) = temp_profile("utf16");
        let original = encode_text("Write-Host user\n", TextEncoding::Utf16Le, "\r\n");
        fs::write(&path, original).unwrap();
        let shell = Shell::from_name("pwsh").unwrap();

        update_profile(&path, shell, SetupMode::Hooks, false).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.starts_with(&[0xff, 0xfe]));
        let (text, encoding) = decode_text(&bytes).unwrap();
        assert_eq!(encoding, TextEncoding::Utf16Le);
        assert!(text.contains("Write-Host user\r\n"));
        assert!(!text.replace("\r\n", "").contains('\n'));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn malformed_profile_is_not_modified() {
        let (path, dir) = temp_profile("malformed");
        let original = format!("{BOOTSTRAP_START}\nmissing end\n");
        fs::write(&path, &original).unwrap();
        let shell = Shell::from_name("bash").unwrap();

        assert!(update_profile(&path, shell, SetupMode::Auto, false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn preserves_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let (path, dir) = temp_profile("permissions");
        fs::write(&path, "echo user\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let shell = Shell::from_name("bash").unwrap();

        update_profile(&path, shell, SetupMode::Auto, false).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn updates_symlink_target_without_replacing_the_link() {
        use std::os::unix::fs::symlink;

        let (link, dir) = temp_profile("symlink");
        let target = dir.join("managed-profile");
        fs::write(&target, "echo user\n").unwrap();
        symlink(&target, &link).unwrap();
        let shell = Shell::from_name("zsh").unwrap();

        update_profile(&link, shell, SetupMode::Hooks, false).unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::read_to_string(&target)
                .unwrap()
                .contains(INTEGRATION_START)
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
