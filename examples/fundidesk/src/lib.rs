//! FundiDesk: a phone-repair shop's app, reproduced from a design.
//!
//! The design is twelve screens exported as HTML (`design/html`), with a
//! screenshot of each (`design/reference`). This crate is the same twelve
//! screens in Morningstar's UI layer, and the test of the framework's
//! purpose: design in, Rust out, the same pixels on screen.
//!
//! How it is laid out:
//!
//! * [`theme`]: the design's tokens (colours, type, radii, spacing).
//! * [`icons`]: every icon of the design, as constants.
//! * [`components`]: the pieces screens share (buttons, fields, pills,
//!   cards, bars).
//! * [`screens`]: one file per screen. Each owns its state, its messages,
//!   its `view` and its `update`, so screens can be written independently.
//! * This file: the [`Screen`] list, the application that hosts one screen
//!   at a time, and navigation between them.
//!
//! `SCREENS.md` is the recipe for building a screen; `design/README.md`
//! says where the reference images came from.

pub mod components;
pub mod icons;
pub mod screens;
pub mod theme;

use morningstar::ui::prelude::*;

use components::Tab;
use screens::{
    create_account, dashboard, login, new_repair, onboarding, record_payment, repair_details,
    repairs_list, shop_setup, splash, tracking, verification,
};

/// The twelve screens of the design, in the design's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Screen {
    /// 01: the logo on green.
    #[default]
    Splash,
    /// 02: the first onboarding page.
    Onboarding,
    /// 03: log in.
    Login,
    /// 04: create an owner account.
    CreateAccount,
    /// 05: enter the verification code.
    Verification,
    /// 06: set up the shop.
    ShopSetup,
    /// 07: the dashboard.
    Dashboard,
    /// 08: the list of repairs.
    RepairsList,
    /// 09: take in a new repair.
    NewRepair,
    /// 10: one repair in detail.
    RepairDetails,
    /// 11: record a payment.
    RecordPayment,
    /// 12: the customer's tracking page.
    Tracking,
}

impl Screen {
    /// Every screen, in the design's order.
    pub const ALL: [Self; 12] = [
        Self::Splash,
        Self::Onboarding,
        Self::Login,
        Self::CreateAccount,
        Self::Verification,
        Self::ShopSetup,
        Self::Dashboard,
        Self::RepairsList,
        Self::NewRepair,
        Self::RepairDetails,
        Self::RecordPayment,
        Self::Tracking,
    ];

    /// The screen's file name in `design/html` and `design/reference`,
    /// without extension: `"07-dashboard"`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Splash => "01-splash",
            Self::Onboarding => "02-onboarding",
            Self::Login => "03-login",
            Self::CreateAccount => "04-create-account",
            Self::Verification => "05-verification",
            Self::ShopSetup => "06-shop-setup",
            Self::Dashboard => "07-dashboard",
            Self::RepairsList => "08-repairs-list",
            Self::NewRepair => "09-new-repair",
            Self::RepairDetails => "10-repair-details",
            Self::RecordPayment => "11-record-payment",
            Self::Tracking => "12-tracking",
        }
    }

    /// Height of the screen's artboard in the design, in dp (its width is
    /// always [`DESIGN_WIDTH`]). Screens taller than a phone show their
    /// whole scrolling content.
    pub const fn design_height(self) -> f32 {
        match self {
            Self::Splash
            | Self::Onboarding
            | Self::Login
            | Self::CreateAccount
            | Self::Verification
            | Self::ShopSetup => 844.0,
            Self::Dashboard => 1060.0,
            Self::RepairsList => 900.0,
            Self::NewRepair => 1260.0,
            Self::RepairDetails => 1380.0,
            Self::RecordPayment => 1040.0,
            Self::Tracking => 1000.0,
        }
    }

    /// True once the screen has been built from the design; false while it
    /// is a placeholder. The fidelity test compares the built ones with
    /// their references. Set by each screen's own file.
    pub const fn is_built(self) -> bool {
        match self {
            Self::Splash => splash::BUILT,
            Self::Onboarding => onboarding::BUILT,
            Self::Login => login::BUILT,
            Self::CreateAccount => create_account::BUILT,
            Self::Verification => verification::BUILT,
            Self::ShopSetup => shop_setup::BUILT,
            Self::Dashboard => dashboard::BUILT,
            Self::RepairsList => repairs_list::BUILT,
            Self::NewRepair => new_repair::BUILT,
            Self::RepairDetails => repair_details::BUILT,
            Self::RecordPayment => record_payment::BUILT,
            Self::Tracking => tracking::BUILT,
        }
    }
}

/// Width of every artboard in the design, in dp.
pub const DESIGN_WIDTH: f32 = 390.0;
/// Height of the phone the design was drawn for, in dp.
pub const DESIGN_HEIGHT: f32 = 844.0;

/// Radius, in dp, of the rounded-corner mask the design tool put on every
/// artboard. The app draws square corners (the device rounds its own
/// screen), so comparisons with the reference images leave the four corner
/// squares of this size out. See [`corner_masks`].
pub const DESIGN_CORNER_RADIUS: f32 = 28.0;

/// The four corner squares of a `width` x `height` pixel render at `scale`
/// pixels per dp that the design's rounded-corner mask touches, each as
/// `(x, y, width, height)` in pixels: the regions to ignore when comparing
/// a render with its reference.
pub fn corner_masks(width: u32, height: u32, scale: f32) -> [(u32, u32, u32, u32); 4] {
    let side = (DESIGN_CORNER_RADIUS * scale).ceil() as u32;
    let (right, bottom) = (width.saturating_sub(side), height.saturating_sub(side));
    [
        (0, 0, side, side),
        (right, 0, side, side),
        (0, bottom, side, side),
        (right, bottom, side, side),
    ]
}

/// What a screen's `update` asks the application to do next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Nav {
    /// Stay on this screen.
    #[default]
    Stay,
    /// Show another screen (the design's links all name their target, the
    /// back arrows included, so there is no history stack).
    To(Screen),
}

/// Where a tab of the tab bar leads. The design links Customers to the
/// repairs list and Account to shop setup.
pub const fn tab_destination(tab: Tab) -> Screen {
    match tab {
        Tab::Home => Screen::Dashboard,
        Tab::Repairs | Tab::Customers => Screen::RepairsList,
        Tab::Account => Screen::ShopSetup,
    }
}

/// Everything a widget can ask for: the message of the screen it is on.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// From the splash screen.
    Splash(splash::Message),
    /// From onboarding.
    Onboarding(onboarding::Message),
    /// From the login screen.
    Login(login::Message),
    /// From account creation.
    CreateAccount(create_account::Message),
    /// From the verification screen.
    Verification(verification::Message),
    /// From shop setup.
    ShopSetup(shop_setup::Message),
    /// From the dashboard.
    Dashboard(dashboard::Message),
    /// From the repairs list.
    RepairsList(repairs_list::Message),
    /// From the new-repair form.
    NewRepair(new_repair::Message),
    /// From a repair's details.
    RepairDetails(repair_details::Message),
    /// From the payment form.
    RecordPayment(record_payment::Message),
    /// From the tracking page.
    Tracking(tracking::Message),
    /// Show a screen directly: for tests and the screenshot tool.
    Open(Screen),
}

/// The application: the screen being shown and each screen's own state.
#[derive(Default)]
pub struct FundiDesk {
    screen: Screen,
    splash: splash::State,
    onboarding: onboarding::State,
    login: login::State,
    create_account: create_account::State,
    verification: verification::State,
    shop_setup: shop_setup::State,
    dashboard: dashboard::State,
    repairs_list: repairs_list::State,
    new_repair: new_repair::State,
    repair_details: repair_details::State,
    record_payment: record_payment::State,
    tracking: tracking::State,
}

impl FundiDesk {
    /// The application showing `screen`.
    pub fn on(screen: Screen) -> Self {
        Self {
            screen,
            ..Self::default()
        }
    }

    /// The screen being shown.
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// What has been typed on the login screen.
    pub fn login(&self) -> &login::State {
        &self.login
    }

    /// What has been typed on the create-account screen.
    pub fn create_account(&self) -> &create_account::State {
        &self.create_account
    }

    /// The verification code entered so far.
    pub fn verification(&self) -> &verification::State {
        &self.verification
    }

    /// The shop-setup form.
    pub fn shop_setup(&self) -> &shop_setup::State {
        &self.shop_setup
    }

    /// The repairs list's search and filter.
    pub fn repairs_list(&self) -> &repairs_list::State {
        &self.repairs_list
    }

    /// The new-repair form.
    pub fn new_repair(&self) -> &new_repair::State {
        &self.new_repair
    }

    /// The payment form.
    pub fn record_payment(&self) -> &record_payment::State {
        &self.record_payment
    }
}

impl Application for FundiDesk {
    type Message = Message;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        // Inter 400 to 700 and Space Grotesk 500 to 700 (SIL OFL; see
        // assets/fonts/README.md). Inter first: it is the fallback for
        // anything Space Grotesk lacks.
        let fonts: [&[u8]; 7] = [
            include_bytes!("../assets/fonts/Inter-Regular.ttf"),
            include_bytes!("../assets/fonts/Inter-Medium.ttf"),
            include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
            include_bytes!("../assets/fonts/Inter-Bold.ttf"),
            include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf"),
            include_bytes!("../assets/fonts/SpaceGrotesk-SemiBold.ttf"),
            include_bytes!("../assets/fonts/SpaceGrotesk-Bold.ttf"),
        ];
        for font in fonts {
            setup.add_font(font);
        }
    }

    fn theme(&self) -> Theme {
        theme::THEME
    }

    /// The splash is the logo on green: light icons in the system's bars
    /// there. Every other screen is on the theme's light canvas, which
    /// gets dark icons by itself (ADR 0026).
    fn system_bars(&self) -> Option<SystemBars> {
        (self.screen == Screen::Splash).then_some(SystemBars::LIGHT_CONTENT)
    }

    fn view(&self) -> Element<Message> {
        match self.screen {
            Screen::Splash => splash::view(&self.splash).map(Message::Splash),
            Screen::Onboarding => onboarding::view(&self.onboarding).map(Message::Onboarding),
            Screen::Login => login::view(&self.login).map(Message::Login),
            Screen::CreateAccount => {
                create_account::view(&self.create_account).map(Message::CreateAccount)
            }
            Screen::Verification => {
                verification::view(&self.verification).map(Message::Verification)
            }
            Screen::ShopSetup => shop_setup::view(&self.shop_setup).map(Message::ShopSetup),
            Screen::Dashboard => dashboard::view(&self.dashboard).map(Message::Dashboard),
            Screen::RepairsList => repairs_list::view(&self.repairs_list).map(Message::RepairsList),
            Screen::NewRepair => new_repair::view(&self.new_repair).map(Message::NewRepair),
            Screen::RepairDetails => {
                repair_details::view(&self.repair_details).map(Message::RepairDetails)
            }
            Screen::RecordPayment => {
                record_payment::view(&self.record_payment).map(Message::RecordPayment)
            }
            Screen::Tracking => tracking::view(&self.tracking).map(Message::Tracking),
        }
    }

    fn update(&mut self, message: Message) {
        let nav = match message {
            Message::Splash(message) => splash::update(&mut self.splash, message),
            Message::Onboarding(message) => onboarding::update(&mut self.onboarding, message),
            Message::Login(message) => login::update(&mut self.login, message),
            Message::CreateAccount(message) => {
                create_account::update(&mut self.create_account, message)
            }
            Message::Verification(message) => verification::update(&mut self.verification, message),
            Message::ShopSetup(message) => shop_setup::update(&mut self.shop_setup, message),
            Message::Dashboard(message) => dashboard::update(&mut self.dashboard, message),
            Message::RepairsList(message) => repairs_list::update(&mut self.repairs_list, message),
            Message::NewRepair(message) => new_repair::update(&mut self.new_repair, message),
            Message::RepairDetails(message) => {
                repair_details::update(&mut self.repair_details, message)
            }
            Message::RecordPayment(message) => {
                record_payment::update(&mut self.record_payment, message)
            }
            Message::Tracking(message) => tracking::update(&mut self.tracking, message),
            Message::Open(screen) => Nav::To(screen),
        };
        if let Nav::To(screen) = nav {
            self.screen = screen;
        }
    }
}

morningstar::app!(Ui::new(FundiDesk::default()));
