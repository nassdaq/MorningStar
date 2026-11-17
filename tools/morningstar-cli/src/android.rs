//! `morningstar build android`: what `tools/build-android.sh` does in the
//! repository, for an app anywhere, with the Android host carried inside
//! this binary (ADR 0045).
//!
//! The host's sources are written to `<target>/morningstar/android-host`
//! of the app's workspace, touching only what differs, so that Gradle has
//! nothing to redo the second time. The host and the app's library are two
//! halves of one interface (`MorningstarNative.java` and the JNI of
//! `morningstar-android`), so the build refuses an app whose framework is
//! of another release than this command.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use morningstar_devtools::{IconSet, Identity, cargo_metadata, write_generated};
use morningstar_preview::runner::Workspace;

use crate::{VERSION, app};

pub const USAGE: &str = "\
Builds the app of this directory for Android and packages it into a debug APK.

Usage: morningstar build android [options]

Options:
  --abis <list>          the ABIs to build, separated by commas
                         (default: arm64-v8a,x86_64, or MORNINGSTAR_ABIS;
                         arm64-v8a alone halves the time and the disk used)
  --install              install the APK on the connected phone and start it
  --manifest-path <file> the app's Cargo.toml, if not this directory's package
  -h, --help             print this help

The app's name, application id, launcher icon and opt-in services come from
[package.metadata.morningstar] in its Cargo.toml. Environment variables (such
as a server address the app reads at compile time) reach the build as they
are.

Needs: the Android SDK and NDK (ANDROID_HOME; ANDROID_NDK_HOME, or the newest
complete NDK under the SDK), cargo-ndk (cargo install cargo-ndk), a JDK 17 or
newer, and for --install adb.
";

/// The Android host's sources, as their path under the host's directory
/// and their content. The same files as `templates/android` of the
/// repository holds in git; a test holds the two lists together.
const HOST: &[(&str, &[u8])] = &[
    (
        "build.gradle.kts",
        include_bytes!("../android/build.gradle.kts"),
    ),
    (
        "settings.gradle.kts",
        include_bytes!("../android/settings.gradle.kts"),
    ),
    (
        "gradle.properties",
        include_bytes!("../android/gradle.properties"),
    ),
    ("gradlew", include_bytes!("../android/gradlew")),
    ("gradlew.bat", include_bytes!("../android/gradlew.bat")),
    (
        "gradle/wrapper/gradle-wrapper.jar",
        include_bytes!("../android/gradle/wrapper/gradle-wrapper.jar"),
    ),
    (
        "gradle/wrapper/gradle-wrapper.properties",
        include_bytes!("../android/gradle/wrapper/gradle-wrapper.properties"),
    ),
    (
        "app/build.gradle.kts",
        include_bytes!("../android/app/build.gradle.kts"),
    ),
    (
        "app/src/main/AndroidManifest.xml",
        include_bytes!("../android/app/src/main/AndroidManifest.xml"),
    ),
    (
        "app/src/main/res/xml/network_security_config.xml",
        include_bytes!("../android/app/src/main/res/xml/network_security_config.xml"),
    ),
    (
        "app/src/main/java/dev/morningstar/host/MorningstarAccessibility.java",
        include_bytes!(
            "../android/app/src/main/java/dev/morningstar/host/MorningstarAccessibility.java"
        ),
    ),
    (
        "app/src/main/java/dev/morningstar/host/MorningstarActivity.java",
        include_bytes!(
            "../android/app/src/main/java/dev/morningstar/host/MorningstarActivity.java"
        ),
    ),
    (
        "app/src/main/java/dev/morningstar/host/MorningstarNative.java",
        include_bytes!("../android/app/src/main/java/dev/morningstar/host/MorningstarNative.java"),
    ),
    (
        "app/src/main/java/dev/morningstar/host/MorningstarServices.java",
        include_bytes!(
            "../android/app/src/main/java/dev/morningstar/host/MorningstarServices.java"
        ),
    ),
    (
        "app/src/live/java/dev/morningstar/host/MorningstarLive.kt",
        include_bytes!("../android/app/src/live/java/dev/morningstar/host/MorningstarLive.kt"),
    ),
    (
        "app/src/live/java/dev/morningstar/host/MorningstarLiveService.kt",
        include_bytes!(
            "../android/app/src/live/java/dev/morningstar/host/MorningstarLiveService.kt"
        ),
    ),
    (
        "app/src/nolive/java/dev/morningstar/host/MorningstarLive.java",
        include_bytes!("../android/app/src/nolive/java/dev/morningstar/host/MorningstarLive.java"),
    ),
];

/// Written beside the host: the release it is of. A host of another
/// release is removed whole before this one is written, so that no file
/// an older release had and this one dropped is built.
const STAMP: &str = ".morningstar-release";

struct Arguments {
    abis: Vec<String>,
    install: bool,
    manifest_path: Option<PathBuf>,
}

fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Option<Arguments>, String> {
    let mut abis = std::env::var("MORNINGSTAR_ABIS")
        .ok()
        .filter(|abis| !abis.trim().is_empty())
        .unwrap_or_else(|| "arm64-v8a,x86_64".to_owned());
    let mut install = false;
    let mut manifest_path = None;
    while let Some(argument) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| format!("{argument} needs a value"))
        };
        match argument.as_str() {
            "-h" | "--help" => return Ok(None),
            "--abis" => abis = value()?,
            "--install" => install = true,
            "--manifest-path" => manifest_path = Some(PathBuf::from(value()?)),
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    const KNOWN: [&str; 4] = ["arm64-v8a", "armeabi-v7a", "x86_64", "x86"];
    let abis: Vec<String> = abis
        .split([',', ' '])
        .map(str::trim)
        .filter(|abi| !abi.is_empty())
        .map(str::to_owned)
        .collect();
    if abis.is_empty() {
        return Err("--abis: name at least one, like arm64-v8a".to_owned());
    }
    if let Some(unknown) = abis.iter().find(|abi| !KNOWN.contains(&abi.as_str())) {
        return Err(format!(
            "--abis: `{unknown}` is not an Android ABI; expected {}",
            KNOWN.join(", ")
        ));
    }
    Ok(Some(Arguments {
        abis,
        install,
        manifest_path,
    }))
}

pub fn main(arguments: impl Iterator<Item = String>) -> ExitCode {
    let arguments = match parse(arguments) {
        Ok(Some(arguments)) => arguments,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("morningstar build android: {error}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match build(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("morningstar build android: {error}");
            ExitCode::FAILURE
        }
    }
}

fn build(arguments: &Arguments) -> Result<(), String> {
    let manifest = match &arguments.manifest_path {
        Some(path) => path.clone(),
        None => app::nearest_package_manifest()?,
    };
    let package = app::current_package(Some(&manifest))?;
    let metadata = cargo_metadata(Some(&manifest)).map_err(|error| error.to_string())?;
    let identity =
        Identity::from_cargo_metadata(&metadata, &package).map_err(|error| error.to_string())?;
    let workspace = Workspace::from_cargo_metadata(&metadata)
        .ok_or("cargo metadata did not say where the workspace is")?;
    let ndk = ndk_home()?;

    let host = workspace.target_dir.join("morningstar/android-host");
    write_host(&host).map_err(|error| format!("{}: {error}", host.display()))?;

    // The launcher icons, into the build directory, never the sources.
    let res_dir = host.join("app/build/generated/morningstar/res");
    let icons = IconSet::for_identity(&identity).map_err(|error| error.to_string())?;
    let files = icons.android_files().map_err(|error| error.to_string())?;
    write_generated(&res_dir, &files).map_err(|error| error.to_string())?;

    // The app's library, one per ABI. Started clean so that an ABI left
    // out this time does not ship a stale library.
    let jni_libs = host.join("app/src/main/jniLibs");
    remove_dir(&jni_libs)?;
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.arg("ndk");
    for abi in &arguments.abis {
        command.args(["--target", abi]);
    }
    command
        .args(["--platform", "24", "--output-dir"])
        .arg(&jni_libs)
        .args(["build", "--package", &package, "--release"])
        .current_dir(&identity.manifest_dir)
        .env("ANDROID_NDK_HOME", &ndk);
    run(&mut command, "cargo ndk (cargo install cargo-ndk)")?;

    // Only now is the lock certain to name the framework's release.
    check_release(&workspace.root.join("Cargo.lock"))?;

    let apk = host.join("app/build/outputs/apk/debug/app-debug.apk");
    // Packaged afresh: Gradle otherwise rewrites the previous APK in place
    // and keeps its size as dead space inside the file.
    let _ = std::fs::remove_file(&apk);
    let gradlew = if cfg!(windows) {
        host.join("gradlew.bat")
    } else {
        host.join("gradlew")
    };
    let services = identity.services.join(",");
    run(
        Command::new(&gradlew)
            .args(["--quiet", "assembleDebug"])
            .arg(format!("-Pmorningstar.appId={}", identity.id))
            .arg(format!("-Pmorningstar.appName={}", identity.name))
            .arg(format!("-Pmorningstar.libName={}", identity.lib))
            .arg(format!("-Pmorningstar.services={services}"))
            .arg(format!("-Pmorningstar.resDir={}", res_dir.display()))
            .current_dir(&host),
        "Gradle (a JDK 17 or newer)",
    )?;
    let size = std::fs::metadata(&apk)
        .map_err(|error| format!("{}: {error}", apk.display()))?
        .len();
    println!(
        "APK: {} ({size} bytes{})",
        apk.display(),
        if services.is_empty() {
            String::new()
        } else {
            format!(", with services: {services}")
        }
    );

    if arguments.install {
        run(Command::new("adb").args(["install", "-r"]).arg(&apk), "adb")?;
        run(
            Command::new("adb").args([
                "shell",
                "am",
                "start",
                "-n",
                &format!("{}/dev.morningstar.host.MorningstarActivity", identity.id),
            ]),
            "adb",
        )?;
        println!("Logs: adb logcat -s Morningstar");
    }
    Ok(())
}

/// Writes the host into `dir`: whole when it is of another release or
/// missing, otherwise only the files that differ.
fn write_host(dir: &Path) -> std::io::Result<()> {
    let stamp = dir.join(STAMP);
    if std::fs::read_to_string(&stamp).is_ok_and(|release| release != VERSION) {
        std::fs::remove_dir_all(dir)?;
    }
    for (path, content) in HOST {
        let target = dir.join(path);
        if std::fs::read(&target).is_ok_and(|existing| existing == *content) {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, content)?;
        #[cfg(unix)]
        if *path == "gradlew" {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    std::fs::write(stamp, VERSION)
}

/// Refuses a lock whose `morningstar-android` is of another release than
/// this command's host.
fn check_release(lock: &Path) -> Result<(), String> {
    let text =
        std::fs::read_to_string(lock).map_err(|error| format!("{}: {error}", lock.display()))?;
    match locked_version(&text, "morningstar-android") {
        Some(version) if version == VERSION => Ok(()),
        Some(version) => Err(format!(
            "the app is built on Morningstar {version} and this command carries the Android \
             host of {VERSION}; the two must be one release. Install the command of the \
             app's release (cargo install morningstar-cli --version {version}) or move the \
             app to {VERSION} in its Cargo.toml"
        )),
        None => Err(format!(
            "{} has no morningstar-android: is this a Morningstar app?",
            lock.display()
        )),
    }
}

/// The version `Cargo.lock` gives the package `name`.
fn locked_version(lock: &str, name: &str) -> Option<String> {
    let wanted = format!("name = \"{name}\"");
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == wanted {
            let version = lines.next()?.trim().strip_prefix("version = \"")?;
            return version.strip_suffix('"').map(str::to_owned);
        }
    }
    None
}

/// `ANDROID_NDK_HOME`, or the newest complete NDK under `ANDROID_HOME`
/// (a partly downloaded one has no `source.properties`).
fn ndk_home() -> Result<PathBuf, String> {
    if let Some(ndk) = std::env::var_os("ANDROID_NDK_HOME").filter(|ndk| !ndk.is_empty()) {
        return Ok(PathBuf::from(ndk));
    }
    let sdk = std::env::var_os("ANDROID_HOME")
        .filter(|sdk| !sdk.is_empty())
        .ok_or("ANDROID_HOME must point at the Android SDK")?;
    let ndks = PathBuf::from(sdk).join("ndk");
    let mut complete: Vec<(Vec<u64>, PathBuf)> = std::fs::read_dir(&ndks)
        .map_err(|error| format!("{}: {error}", ndks.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("source.properties").is_file())
        .map(|path| {
            let version = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .split('.')
                .map(|part| part.parse().unwrap_or(0))
                .collect();
            (version, path)
        })
        .collect();
    complete.sort();
    complete
        .pop()
        .map(|(_, path)| path)
        .ok_or_else(|| format!("no complete Android NDK under {}", ndks.display()))
}

fn remove_dir(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{}: {error}", dir.display())),
    }
}

fn run(command: &mut Command, what: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("{what} could not be started: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{what} failed ({status})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The files git holds under `templates/android`, when this is built in
    /// the repository.
    fn tracked() -> Option<Vec<String>> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = Command::new("git")
            .args(["ls-files", "templates/android"])
            .current_dir(&root)
            .output()
            .ok()?;
        output.status.success().then(|| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| line.strip_prefix("templates/android/"))
                .map(str::to_owned)
                .collect()
        })
    }

    #[test]
    fn the_host_carried_is_the_host_in_git() {
        let Some(mut tracked) = tracked() else {
            return; // Not a checkout: nothing to compare with.
        };
        let mut carried: Vec<String> = HOST.iter().map(|(path, _)| (*path).to_owned()).collect();
        tracked.sort();
        carried.sort();
        assert_eq!(
            carried, tracked,
            "a file was added to or removed from templates/android: change HOST, and the \
             `include` list of this crate's Cargo.toml, with it"
        );
    }

    #[test]
    fn the_host_is_written_once_and_replaced_whole_for_another_release() {
        let dir = std::env::temp_dir().join(format!("ms-cli-host-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_host(&dir).unwrap();
        let gradlew = dir.join("gradlew");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&gradlew).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "gradlew can be run");
        }
        let first = std::fs::metadata(&gradlew).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_host(&dir).unwrap();
        assert_eq!(
            std::fs::metadata(&gradlew).unwrap().modified().unwrap(),
            first,
            "nothing that is the same is written again"
        );
        // A host of another release, with a file this one does not have.
        std::fs::write(dir.join(STAMP), "0.0.0").unwrap();
        std::fs::write(dir.join("app/Stale.java"), "x").unwrap();
        write_host(&dir).unwrap();
        assert!(!dir.join("app/Stale.java").exists());
        assert_eq!(std::fs::read_to_string(dir.join(STAMP)).unwrap(), VERSION);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_locked_release_is_read_and_held_to_this_one() {
        let lock = format!(
            "[[package]]\nname = \"morningstar-core\"\nversion = \"9.9.9\"\n\n\
             [[package]]\nname = \"morningstar-android\"\nversion = \"{VERSION}\"\n"
        );
        assert_eq!(
            locked_version(&lock, "morningstar-android").as_deref(),
            Some(VERSION)
        );
        assert_eq!(locked_version(&lock, "absent"), None);
        let dir = std::env::temp_dir().join(format!("ms-cli-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Cargo.lock");
        std::fs::write(&path, &lock).unwrap();
        assert!(check_release(&path).is_ok());
        std::fs::write(
            &path,
            "[[package]]\nname = \"morningstar-android\"\nversion = \"0.0.9\"\n",
        )
        .unwrap();
        let error = check_release(&path).unwrap_err();
        assert!(
            error.contains("cargo install morningstar-cli --version 0.0.9"),
            "{error}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn abis_are_checked() {
        let parse = |args: &[&str]| parse(args.iter().map(|arg| (*arg).to_owned()));
        let parsed = parse(&["--abis", "arm64-v8a", "--install"])
            .unwrap()
            .unwrap();
        assert_eq!(parsed.abis, ["arm64-v8a"]);
        assert!(parsed.install);
        assert!(parse(&["--abis", "arm64"]).is_err());
        assert!(parse(&["--abis", ""]).is_err());
        assert!(parse(&["--what"]).is_err());
    }
}
