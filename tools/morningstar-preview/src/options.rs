//! The command line, shared by the supervisor and the host it starts.

use std::path::PathBuf;

/// The size of the previewed screen in dp: an ordinary phone, and the size
/// the design references of the examples are drawn at.
pub const DEFAULT_SIZE: (f32, f32) = (390.0, 844.0);

/// The safe-area insets in dp when none are given: the status bar and the
/// gesture bar the examples' design references are compared with (102 and
/// 42 px at 2 px per dp).
pub const DEFAULT_INSETS: Edges = Edges {
    left: 0.0,
    top: 51.0,
    right: 0.0,
    bottom: 21.0,
};

/// Four distances in dp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Edges {
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
}

/// One step of a scripted run: what a person would do, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Tap at this point, in dp from the screen's top-left corner.
    Tap(f32, f32),
    /// Type this into the focused field.
    Type(String),
    /// The system's back (the Escape key).
    Back,
    /// Wait this many more milliseconds before the next step.
    Wait(u64),
}

/// The pause before each scripted step, so that the app has drawn what the
/// step before it changed.
pub const STEP_PAUSE_MS: u64 = 500;

/// Everything the command line can say.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// The Cargo package of the app. Empty only for the host, which is
    /// already linked to its app.
    pub app: String,
    /// The app's `Cargo.toml`, or one of its workspace; Cargo looks upwards
    /// from the current directory without it.
    pub manifest_path: Option<PathBuf>,
    /// The screen in dp.
    pub size: (f32, f32),
    /// Physical pixels per dp. `None`: the display's own, made smaller if
    /// the window would not fit on it.
    pub scale: Option<f32>,
    /// The safe area's insets in dp.
    pub insets: Edges,
    /// The height in dp the soft keyboard is imitated with while a field
    /// has the focus; zero for none.
    pub keyboard: f32,
    /// Where the window's top-left corner goes, in screen points. `None`:
    /// where it last was, or where the system puts it.
    pub position: Option<(i32, i32)>,
    /// Start as if the user had asked the system for less motion.
    pub reduce_motion: bool,
    /// Build and run once, without watching the sources.
    pub no_watch: bool,
    /// Forget the stored values first: the app's first launch.
    pub fresh: bool,
    /// Save a picture of the screen after this many milliseconds, then quit.
    pub screenshot_after: Option<(u64, PathBuf)>,
    /// What to do in the window by itself, in order (`--tap`, `--type`,
    /// `--back`, `--wait`): for scripts and for pictures of a screen that
    /// is a few taps in.
    pub script: Vec<Step>,
    /// Host only: where the stored values and the window's place are kept.
    pub state_dir: Option<PathBuf>,
    /// Host only: the app's name, for the title bar.
    pub title: Option<String>,
    /// Host only: do not take the keyboard focus (a reload must not pull
    /// the focus out of the editor).
    pub background: bool,
    /// Host only: standard input is the supervisor's, which says how the
    /// build stands and when to quit; its end means the supervisor is gone.
    pub supervised: bool,
    /// `--help` was given.
    pub help: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            app: String::new(),
            manifest_path: None,
            size: DEFAULT_SIZE,
            scale: None,
            insets: DEFAULT_INSETS,
            keyboard: 0.0,
            position: None,
            reduce_motion: false,
            no_watch: false,
            fresh: false,
            screenshot_after: None,
            script: Vec::new(),
            state_dir: None,
            title: None,
            background: false,
            supervised: false,
            help: false,
        }
    }
}

/// What `--help` prints.
pub const USAGE: &str = "\
Runs a Morningstar app in a window on this Mac and reloads it when its source changes.

  morningstar-preview <app> [options]        (tools/preview.sh <app> [options])

<app> is the Cargo package of the app, such as barua-talk.

Options:
  --size <W>x<H>            the screen in dp (default 390x844)
  --scale <S>               physical pixels per dp (default: this display's,
                            less if the window would not fit on it)
  --insets <T>[,<B>]        the safe area's top and bottom insets in dp, or
                            <L>,<T>,<R>,<B> (default 51,21: a status bar and a
                            gesture bar; 0 for none)
  --keyboard <H>            imitate a soft keyboard this many dp high while a
                            text field has the focus (default 0: none)
  --position <X>,<Y>        the window's top-left corner in screen points, for
                            the first start (default: where it last was)
  --reduce-motion           start with the system's \"reduce motion\" on
  --fresh                   forget the stored values first: a first launch
  --no-watch                build and run once; do not reload
  --screenshot-after <MS> <PATH>
                            save the screen as a PNG after MS milliseconds and
                            quit (implies --no-watch)
  --tap <X>,<Y>             tap at this point in dp by itself; with --type <TEXT>
  --type <TEXT>             (into the focused field), --back and --wait <MS>,
  --back, --wait <MS>       a script: steps run in the order given, half a
                            second apart, starting when the app has drawn
  --manifest-path <PATH>    the app's Cargo.toml, if it is not in or under the
                            current directory's workspace
  --help                    this text

Keys, in the window:
  the mouse is a finger; the wheel or two fingers on the trackpad scroll
  typing goes to the focused text field; Cmd+A/C/X/V/Z and the arrows work
  Esc            the system's back (with a field focused: put the keyboard away)
  Cmd+S or F2    save the screen as a PNG under target/preview/<app>/
  Cmd+R or F5    start the app again; with Shift, as on its first launch
  F3             turn \"reduce motion\" on and off
  F4             turn the safe-area insets on and off
  F1             print this list
";

impl Options {
    /// Reads the arguments after the program's name.
    pub fn parse<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut options = Self::default();
        let mut args = args.into_iter().map(Into::into);
        while let Some(arg) = args.next() {
            let mut value = |name: &str| {
                args.next()
                    .ok_or_else(|| format!("{name} needs a value; see --help"))
            };
            match arg.as_str() {
                "--help" | "-h" => options.help = true,
                "--size" => options.size = parse_size(&value("--size")?)?,
                "--scale" => {
                    let text = value("--scale")?;
                    let scale = number(&text, "--scale")?;
                    if !(0.25..=8.0).contains(&scale) {
                        return Err(format!("--scale {text}: give 0.25 to 8 pixels per dp"));
                    }
                    options.scale = Some(scale);
                }
                "--insets" => options.insets = parse_insets(&value("--insets")?)?,
                "--keyboard" => {
                    let text = value("--keyboard")?;
                    let height = number(&text, "--keyboard")?;
                    if !(0.0..=2000.0).contains(&height) {
                        return Err(format!("--keyboard {text}: give a height in dp"));
                    }
                    options.keyboard = height;
                }
                "--position" => {
                    let text = value("--position")?;
                    options.position =
                        Some(parse_position(&text).ok_or_else(|| {
                            format!("--position {text}: give X,Y in screen points")
                        })?);
                }
                "--reduce-motion" => options.reduce_motion = true,
                "--no-watch" => options.no_watch = true,
                "--fresh" => options.fresh = true,
                "--background" => options.background = true,
                "--supervised" => options.supervised = true,
                "--screenshot-after" => {
                    let text = value("--screenshot-after")?;
                    let after = text.parse::<u64>().map_err(|_| {
                        format!("--screenshot-after {text}: give milliseconds, then a path")
                    })?;
                    let path = value("--screenshot-after")?;
                    options.screenshot_after = Some((after, PathBuf::from(path)));
                    options.no_watch = true;
                }
                "--tap" => {
                    let text = value("--tap")?;
                    let point = text
                        .split_once(',')
                        .and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)))
                        .filter(|(x, y): &(f32, f32)| x.is_finite() && y.is_finite())
                        .ok_or_else(|| format!("--tap {text}: give X,Y in dp"))?;
                    options.script.push(Step::Tap(point.0, point.1));
                }
                "--type" => options.script.push(Step::Type(value("--type")?)),
                "--back" => options.script.push(Step::Back),
                "--wait" => {
                    let text = value("--wait")?;
                    let wait = text
                        .parse::<u64>()
                        .map_err(|_| format!("--wait {text}: give milliseconds"))?;
                    options.script.push(Step::Wait(wait));
                }
                "--manifest-path" => {
                    options.manifest_path = Some(PathBuf::from(value("--manifest-path")?));
                }
                "--state-dir" => options.state_dir = Some(PathBuf::from(value("--state-dir")?)),
                "--title" => options.title = Some(value("--title")?),
                other if other.starts_with('-') => {
                    return Err(format!("unknown option {other}; see --help"));
                }
                _ if options.app.is_empty() => options.app = arg,
                other => {
                    return Err(format!(
                        "one app at a time: {} and {other} were both given",
                        options.app
                    ));
                }
            }
        }
        Ok(options)
    }

    /// The arguments that make a host look like this: what the supervisor
    /// passes on. The app, the manifest and the watching are the
    /// supervisor's own.
    pub fn host_args(&self) -> Vec<String> {
        let mut args = vec![
            "--size".to_owned(),
            format!("{}x{}", self.size.0, self.size.1),
            "--insets".to_owned(),
            format!(
                "{},{},{},{}",
                self.insets.left, self.insets.top, self.insets.right, self.insets.bottom
            ),
            "--keyboard".to_owned(),
            self.keyboard.to_string(),
        ];
        if let Some(scale) = self.scale {
            args.extend(["--scale".to_owned(), scale.to_string()]);
        }
        if let Some((x, y)) = self.position {
            args.extend(["--position".to_owned(), format!("{x},{y}")]);
        }
        if self.reduce_motion {
            args.push("--reduce-motion".to_owned());
        }
        for step in &self.script {
            match step {
                Step::Tap(x, y) => args.extend(["--tap".to_owned(), format!("{x},{y}")]),
                Step::Type(text) => args.extend(["--type".to_owned(), text.clone()]),
                Step::Back => args.push("--back".to_owned()),
                Step::Wait(wait) => args.extend(["--wait".to_owned(), wait.to_string()]),
            }
        }
        if let Some((after, path)) = &self.screenshot_after {
            args.extend([
                "--screenshot-after".to_owned(),
                after.to_string(),
                path.display().to_string(),
            ]);
        }
        args
    }
}

fn number(text: &str, name: &str) -> Result<f32, String> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{name} {text}: not a number"))
}

fn parse_size(text: &str) -> Result<(f32, f32), String> {
    let bad = || format!("--size {text}: give WIDTHxHEIGHT in dp, such as 390x844");
    let (width, height) = text.split_once(['x', 'X']).ok_or_else(bad)?;
    let width = number(width, "--size").map_err(|_| bad())?;
    let height = number(height, "--size").map_err(|_| bad())?;
    if !(100.0..=4000.0).contains(&width) || !(100.0..=4000.0).contains(&height) {
        return Err(format!("--size {text}: each side is 100 to 4000 dp"));
    }
    Ok((width, height))
}

fn parse_insets(text: &str) -> Result<Edges, String> {
    let bad = || format!("--insets {text}: give TOP, TOP,BOTTOM or LEFT,TOP,RIGHT,BOTTOM in dp");
    let values = text
        .split(',')
        .map(|part| number(part, "--insets").map_err(|_| bad()))
        .collect::<Result<Vec<f32>, String>>()?;
    if values.iter().any(|value| !(0.0..=1000.0).contains(value)) {
        return Err(bad());
    }
    match values[..] {
        [top] => Ok(Edges { top, ..Edges::ZERO }),
        [top, bottom] => Ok(Edges {
            top,
            bottom,
            ..Edges::ZERO
        }),
        [left, top, right, bottom] => Ok(Edges {
            left,
            top,
            right,
            bottom,
        }),
        _ => Err(bad()),
    }
}

/// `X,Y` as two whole numbers.
pub fn parse_position(text: &str) -> Option<(i32, i32)> {
    let (x, y) = text.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// How big the window is and how many pixels a dp gets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// The window's content in the window system's points.
    pub points: (f64, f64),
    /// Physical pixels per dp: what the engine is told.
    pub scale: f32,
    /// True if the screen was made smaller than asked to fit the display.
    pub shrunk: bool,
}

/// Works out the window for a screen of `size` dp.
///
/// `factor` is the display's pixels per point (2 on a Retina display).
/// With no `scale` asked for, a dp is a point, unless the window would be
/// taller than `available_height` points (the display less its menu bar,
/// the title bar and a margin), in which case everything is made smaller
/// by the same amount: the screen keeps its size in dp, and the layout is
/// the phone's. A scale that was asked for is kept whatever the display.
pub fn fit(
    size: (f32, f32),
    scale: Option<f32>,
    factor: f64,
    available_height: Option<f64>,
) -> Fit {
    let factor = if factor.is_finite() && factor > 0.0 {
        factor
    } else {
        1.0
    };
    let (width, height) = (f64::from(size.0), f64::from(size.1));
    let mut zoom = scale.map_or(1.0, |scale| f64::from(scale) / factor);
    let mut shrunk = false;
    if scale.is_none()
        && let Some(available) = available_height.filter(|available| *available >= 200.0)
        && height * zoom > available
    {
        zoom = available / height;
        shrunk = true;
    }
    Fit {
        points: (width * zoom, height * zoom),
        scale: (zoom * factor) as f32,
        shrunk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().copied())
    }

    #[test]
    fn an_app_alone_gets_the_defaults() {
        let options = parse(&["barua-talk"]).unwrap();
        assert_eq!(options.app, "barua-talk");
        assert_eq!(options.size, (390.0, 844.0));
        assert_eq!(options.insets, DEFAULT_INSETS);
        assert_eq!(options.scale, None);
        assert!(!options.no_watch && !options.fresh && !options.help);
    }

    #[test]
    fn every_option_is_read() {
        let options = parse(&[
            "--size",
            "360x800",
            "atlas",
            "--scale",
            "2.8125",
            "--insets",
            "36,15",
            "--keyboard",
            "300",
            "--position",
            "40,-12",
            "--reduce-motion",
            "--fresh",
            "--screenshot-after",
            "1500",
            "out/shot.png",
            "--manifest-path",
            "../app/Cargo.toml",
        ])
        .unwrap();
        assert_eq!(options.app, "atlas");
        assert_eq!(options.size, (360.0, 800.0));
        assert_eq!(options.scale, Some(2.8125));
        assert_eq!(options.insets.top, 36.0);
        assert_eq!(options.insets.bottom, 15.0);
        assert_eq!(options.keyboard, 300.0);
        assert_eq!(options.position, Some((40, -12)));
        assert!(options.reduce_motion && options.fresh);
        assert_eq!(
            options.screenshot_after,
            Some((1500, PathBuf::from("out/shot.png")))
        );
        assert!(options.no_watch, "a screenshot run does not watch");
        assert_eq!(
            options.manifest_path,
            Some(PathBuf::from("../app/Cargo.toml"))
        );
    }

    #[test]
    fn insets_take_one_two_or_four_values() {
        assert_eq!(parse(&["a", "--insets", "0"]).unwrap().insets, Edges::ZERO);
        let four = parse(&["a", "--insets", "1,2,3,4"]).unwrap().insets;
        assert_eq!(
            four,
            Edges {
                left: 1.0,
                top: 2.0,
                right: 3.0,
                bottom: 4.0
            }
        );
        assert!(parse(&["a", "--insets", "1,2,3"]).is_err());
        assert!(parse(&["a", "--insets", "-4"]).is_err());
    }

    #[test]
    fn mistakes_are_explained() {
        assert!(
            parse(&["a", "--size", "390"])
                .unwrap_err()
                .contains("WIDTHxHEIGHT")
        );
        assert!(parse(&["a", "--size", "10x10"]).is_err());
        assert!(parse(&["a", "--scale", "0"]).is_err());
        assert!(
            parse(&["a", "--scale"])
                .unwrap_err()
                .contains("needs a value")
        );
        assert!(
            parse(&["a", "--nope"])
                .unwrap_err()
                .contains("unknown option")
        );
        assert!(
            parse(&["a", "b"])
                .unwrap_err()
                .contains("one app at a time")
        );
        assert!(parse(&["a", "--screenshot-after", "soon", "x.png"]).is_err());
        assert!(
            parse(&["a", "--tap", "12"])
                .unwrap_err()
                .contains("X,Y in dp")
        );
        assert!(parse(&["a", "--wait", "a while"]).is_err());
    }

    #[test]
    fn the_host_is_told_what_the_supervisor_was() {
        let given = parse(&[
            "barua-talk",
            "--size",
            "360x800",
            "--scale",
            "3",
            "--insets",
            "30,10",
            "--keyboard",
            "280",
            "--position",
            "5,6",
            "--reduce-motion",
            "--screenshot-after",
            "200",
            "a b.png",
            "--tap",
            "195,470.5",
            "--type",
            "Neema Juma",
            "--wait",
            "900",
            "--back",
        ])
        .unwrap();
        assert_eq!(
            given.script,
            vec![
                Step::Tap(195.0, 470.5),
                Step::Type("Neema Juma".to_owned()),
                Step::Wait(900),
                Step::Back,
            ]
        );
        let host = Options::parse(given.host_args()).unwrap();
        assert_eq!(host.app, "", "the host is already linked to its app");
        assert_eq!(host.size, given.size);
        assert_eq!(host.scale, given.scale);
        assert_eq!(host.insets, given.insets);
        assert_eq!(host.keyboard, given.keyboard);
        assert_eq!(host.position, given.position);
        assert_eq!(host.reduce_motion, given.reduce_motion);
        assert_eq!(host.screenshot_after, given.screenshot_after);
        assert_eq!(host.script, given.script, "in the same order");
    }

    #[test]
    fn a_dp_is_a_point_when_the_screen_fits() {
        let fit = fit((390.0, 844.0), None, 2.0, Some(900.0));
        assert_eq!(fit.points, (390.0, 844.0));
        assert_eq!(fit.scale, 2.0);
        assert!(!fit.shrunk);
    }

    #[test]
    fn a_screen_taller_than_the_display_is_made_smaller_whole() {
        let fit = fit((390.0, 844.0), None, 2.0, Some(633.0));
        assert!(fit.shrunk);
        assert_eq!(fit.points.1, 633.0);
        assert!((fit.points.0 - 292.5).abs() < 1e-9);
        assert!((fit.scale - 1.5).abs() < 1e-6, "{}", fit.scale);
    }

    #[test]
    fn a_scale_that_was_asked_for_is_kept() {
        let fit = fit((390.0, 844.0), Some(2.8125), 2.0, Some(633.0));
        assert!(!fit.shrunk);
        assert_eq!(fit.scale, 2.8125);
        assert!((fit.points.1 - 844.0 * 2.8125 / 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_display_that_reports_nonsense_counts_as_one_pixel_per_point() {
        let fit = fit((390.0, 844.0), None, f64::NAN, None);
        assert_eq!(fit.scale, 1.0);
    }
}
