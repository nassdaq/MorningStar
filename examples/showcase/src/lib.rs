//! Three realistic screens written only with Morningstar's UI layer: a
//! mobile-money wallet home, a settings list and a sign-in form.
//!
//! This file is meant to be imitated. Its shape is the shape of every app
//! built on the UI layer:
//!
//! 1. a state struct ([`Showcase`]),
//! 2. a message enum ([`Message`]),
//! 3. `view`, which describes the screen for the current state out of small
//!    functions that each return a piece of it,
//! 4. `update`, which changes the state.
//!
//! All lengths are dp. Colours come from the theme (`ThemeColor::..`) or
//! from the palette constants below.
//!
//! Two things the framework cannot do yet show here:
//!
//! * **No text input.** The sign-in fields are drawn, not editable.
//! * **No vector icons.** Icons are short glyphs or shapes built from
//!   boxes. Images work: the profile picture is an image registered in
//!   `setup` (drawn in code here, so the repository carries no binary; an
//!   app would `include_bytes!` a PNG and call `setup.add_image_png`).

use morningstar::ui::prelude::*;

/// Brand palette, as hex values straight from a design.
const INDIGO: Color = hex(0x2B3FE0);
const VIOLET: Color = hex(0x7B3FF2);
const INK: Color = hex(0x131722);
const MINT: Color = hex(0xE3F6EC);
const AMBER: Color = hex(0xFFF1D6);
const AMBER_INK: Color = hex(0xB26A00);
const ROSE: Color = hex(0xFDE7E7);
const SKY: Color = hex(0xE2E9FF);

/// The app's theme: the default light theme with the brand's accent.
const THEME: Theme = Theme {
    colors: Colors {
        primary: INDIGO,
        text: INK,
        ..Theme::LIGHT.colors
    },
    ..Theme::LIGHT
};

/// Screen margin.
const MARGIN: f32 = 20.0;

/// Which screen is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    /// Wallet home.
    #[default]
    Home,
    /// Settings and profile.
    Settings,
    /// Sign-in form.
    Login,
}

/// Interface language. The whole UI is a function of state, so switching
/// language is one field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    /// Kiswahili.
    #[default]
    Swahili,
    /// English.
    English,
}

/// Everything a widget can ask the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// Navigate to a screen.
    Open(Screen),
    /// Show or hide the balance.
    ToggleBalance,
    /// Switch between Swahili and English.
    ToggleLanguage,
    /// Flip the notifications switch.
    ToggleNotifications,
    /// Flip the biometrics switch.
    ToggleBiometrics,
    /// One of the quick actions under the balance card.
    QuickAction(u8),
    /// A transaction row was tapped.
    OpenTransaction(usize),
    /// A row or button that has no behaviour in this demo.
    Nothing,
}

/// The application state.
#[derive(Debug)]
pub struct Showcase {
    /// The screen being shown.
    pub screen: Screen,
    /// Interface language.
    pub language: Language,
    /// Whether the balance is hidden behind dots.
    pub balance_hidden: bool,
    /// Settings switch.
    pub notifications: bool,
    /// Settings switch.
    pub biometrics: bool,
    /// The last transaction tapped, shown highlighted.
    pub selected_transaction: Option<usize>,
    /// How many quick actions have been tapped (shown nowhere; for tests).
    pub quick_actions: u32,
    /// The profile picture, registered in `setup`. `None` until then, and
    /// the avatar falls back to initials.
    pub portrait: Option<Image>,
}

impl Default for Showcase {
    fn default() -> Self {
        Self {
            screen: Screen::Home,
            language: Language::Swahili,
            balance_hidden: false,
            notifications: true,
            biometrics: false,
            selected_transaction: None,
            quick_actions: 0,
            portrait: None,
        }
    }
}

impl Showcase {
    /// An app that starts on `screen`.
    pub fn on(screen: Screen) -> Self {
        Self {
            screen,
            ..Self::default()
        }
    }

    /// Picks the string for the current language.
    fn tr(&self, swahili: &'static str, english: &'static str) -> &'static str {
        match self.language {
            Language::Swahili => swahili,
            Language::English => english,
        }
    }
}

impl Application for Showcase {
    type Message = Message;

    fn setup(&mut self, setup: &mut Setup<'_>) {
        // Primary font first, then fonts for other scripts.
        setup.add_font(include_bytes!(
            "../../hello-gpu/assets/fonts/NotoSans-Regular.ttf"
        ));
        setup.add_font(include_bytes!(
            "../../hello-gpu/assets/fonts/NotoSans-Bold.ttf"
        ));
        setup.add_font(include_bytes!(
            "../../hello-gpu/assets/fonts/NotoSansArabic-Regular.ttf"
        ));
        // Images are registered once, here, and the handle kept in state.
        self.portrait = setup
            .add_image_rgba8(PORTRAIT_SIZE, PORTRAIT_SIZE, &portrait_pixels())
            .ok();
    }

    fn theme(&self) -> Theme {
        THEME
    }

    fn view(&self) -> Element<Message> {
        match self.screen {
            Screen::Home => self.home(),
            Screen::Settings => self.settings(),
            Screen::Login => self.login(),
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Open(screen) => self.screen = screen,
            Message::ToggleBalance => self.balance_hidden = !self.balance_hidden,
            Message::ToggleLanguage => {
                self.language = match self.language {
                    Language::Swahili => Language::English,
                    Language::English => Language::Swahili,
                };
            }
            Message::ToggleNotifications => self.notifications = !self.notifications,
            Message::ToggleBiometrics => self.biometrics = !self.biometrics,
            Message::QuickAction(_) => self.quick_actions += 1,
            Message::OpenTransaction(index) => self.selected_transaction = Some(index),
            Message::Nothing => {}
        }
    }
}

/// Side of the generated profile picture, in pixels.
const PORTRAIT_SIZE: u32 = 144;

/// Draws a stand-in profile picture: a head-and-shoulders silhouette on a
/// warm gradient. RGBA, sRGB-encoded, row by row from the top.
fn portrait_pixels() -> Vec<u8> {
    let size = PORTRAIT_SIZE as f32;
    let mix = |from: [f32; 3], to: [f32; 3], t: f32| {
        [
            from[0] + (to[0] - from[0]) * t,
            from[1] + (to[1] - from[1]) * t,
            from[2] + (to[2] - from[2]) * t,
        ]
    };
    // How much of a pixel at distance `d` outside a shape's edge is covered.
    let coverage = |distance: f32| (0.5 - distance).clamp(0.0, 1.0);
    let mut pixels = Vec::with_capacity((PORTRAIT_SIZE * PORTRAIT_SIZE * 4) as usize);
    for y in 0..PORTRAIT_SIZE {
        for x in 0..PORTRAIT_SIZE {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let background = mix([255.0, 205.0, 150.0], [244.0, 132.0, 120.0], py / size);
            let head =
                ((px - size * 0.5).powi(2) + (py - size * 0.40).powi(2)).sqrt() - size * 0.19;
            let shoulders =
                ((px - size * 0.5).powi(2) + (py - size * 1.08).powi(2)).sqrt() - size * 0.42;
            let figure = coverage(head).max(coverage(shoulders));
            let color = mix(background, [74.0, 38.0, 110.0], figure);
            pixels.extend_from_slice(&[color[0] as u8, color[1] as u8, color[2] as u8, 255]);
        }
    }
    pixels
}

// ------------------------------------------------------------------ home

/// One row of the transaction list.
struct Transaction {
    name: &'static str,
    swahili: &'static str,
    english: &'static str,
    /// Thousands of shillings; negative is money out.
    amount: i32,
}

const TRANSACTIONS: [Transaction; 10] = [
    Transaction {
        name: "Juma Bakari",
        swahili: "Umepokea · Leo 09:41",
        english: "Received · Today 09:41",
        amount: 45_000,
    },
    Transaction {
        name: "LUKU Umeme",
        swahili: "Malipo ya bili · Leo 08:15",
        english: "Bill · Today 08:15",
        amount: -20_000,
    },
    Transaction {
        name: "Neema Mushi",
        swahili: "Umetuma · Jana 18:30",
        english: "Sent · Yesterday 18:30",
        amount: -12_500,
    },
    Transaction {
        name: "Vodacom Bando",
        swahili: "Kifurushi · Jana 07:02",
        english: "Bundle · Yesterday 07:02",
        amount: -3_000,
    },
    Transaction {
        name: "Kilimo Bora Ltd",
        swahili: "Mshahara · 28 Okt",
        english: "Salary · 28 Oct",
        amount: 850_000,
    },
    Transaction {
        name: "Mama Lishe",
        swahili: "Lipa kwa simu · 27 Okt",
        english: "Merchant · 27 Oct",
        amount: -4_500,
    },
    Transaction {
        name: "DAWASA Maji",
        swahili: "Malipo ya bili · 26 Okt",
        english: "Bill payment · 26 Oct",
        amount: -15_200,
    },
    Transaction {
        name: "Baraka Mwakyusa",
        swahili: "Umepokea · 25 Okt",
        english: "Received · 25 Oct",
        amount: 30_000,
    },
    Transaction {
        name: "Azam TV",
        swahili: "Malipo ya bili · 24 Okt",
        english: "Bill payment · 24 Oct",
        amount: -23_000,
    },
    Transaction {
        name: "Benki ya NMB",
        swahili: "Kutoka benki · 22 Okt",
        english: "From bank · 22 Oct",
        amount: 200_000,
    },
];

/// `1250400` as `1,250,400`.
fn grouped(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

impl Showcase {
    fn home(&self) -> Element<Message> {
        col![
            self.home_header(),
            ScrollView::new(
                col![
                    self.balance_card(),
                    self.quick_actions(),
                    self.transactions(),
                ]
                .gap(24.0)
                .padding_ltrb(MARGIN, 14.0, MARGIN, 24.0),
            )
            .id("home-scroll"),
            self.tab_bar(),
        ]
        // An id on the screen root keeps this screen's widget state (its
        // scroll position) apart from the other screens'.
        .id("home")
        .into()
    }

    /// Greeting, avatar and the language switch. Its padding includes the
    /// status bar, so the screen background runs under it.
    fn home_header(&self) -> Container<Message> {
        row![
            Avatar::new("Amina Juma")
                .image_maybe(self.portrait)
                .size(44.0)
                .id("avatar"),
            col![
                text(self.tr("Habari za asubuhi,", "Good morning,"))
                    .caption()
                    .muted(),
                text("Amina Juma").headline(),
            ]
            .gap(1.0)
            .fill_width(),
            // A pill-shaped toggle built from a box and a text.
            Pressable::new(text(self.tr("SW", "EN")).label().color(ThemeColor::Primary))
                .padding_xy(14.0, 8.0)
                .radius(THEME.radii.full)
                .background(ThemeColor::Surface)
                .border(1.0, ThemeColor::Divider)
                .on_press(Message::ToggleLanguage)
                .id("language"),
        ]
        .gap(12.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_ltrb(MARGIN, 12.0, MARGIN, 6.0)
        .safe_area(SafeEdges::TOP)
    }

    /// The hero card: gradient, tinted shadow, decorative circles clipped to
    /// the card's rounded corners, and the balance.
    fn balance_card(&self) -> Container<Message> {
        let white = Color::WHITE;
        let soft = with_alpha(white, 0.72);
        let balance = if self.balance_hidden {
            "TSh •••••••".to_owned()
        } else {
            format!("TSh {}", grouped(1_250_400))
        };

        stack![
            // Decoration. The layer fills the card, so it adds nothing to
            // the card's own size; the circles overhang it and are clipped.
            stack![
                Container::empty()
                    .size(170.0, 170.0)
                    .radius(85.0)
                    .background(with_alpha(white, 0.10))
                    .place(Alignment::TOP_RIGHT)
                    .offset(50.0, -70.0),
                Container::empty()
                    .size(130.0, 130.0)
                    .radius(65.0)
                    .background(with_alpha(white, 0.07))
                    .place(Alignment::BOTTOM_LEFT)
                    .offset(-40.0, 60.0),
            ]
            .fill(),
            col![
                row![
                    text(self.tr("Salio kuu", "Main balance"))
                        .label()
                        .color(soft),
                    Spacer::new(),
                    Container::new(text("TZS").caption().semibold().color(white))
                        .padding_xy(10.0, 4.0)
                        .radius(THEME.radii.full)
                        .background(with_alpha(white, 0.18)),
                ]
                .cross_align(CrossAlign::Center)
                .fill_width(),
                text(balance).display().color(white).id("balance"),
                Spacer::fixed(18.0),
                row![
                    col![
                        text(self.tr("Namba ya akaunti", "Account number"))
                            .caption()
                            .color(soft),
                        text("0712 •••• 4821").label().color(white),
                    ]
                    .gap(2.0),
                    Spacer::new(),
                    Pressable::new(
                        text(if self.balance_hidden {
                            self.tr("Onyesha", "Show")
                        } else {
                            self.tr("Ficha", "Hide")
                        })
                        .label()
                        .color(INDIGO),
                    )
                    .padding_xy(16.0, 9.0)
                    .radius(THEME.radii.full)
                    .background(white)
                    .on_press(Message::ToggleBalance)
                    .id("toggle-balance"),
                ]
                .cross_align(CrossAlign::End)
                .fill_width(),
            ]
            .gap(6.0)
            .padding(MARGIN)
            .fill_width(),
        ]
        .fill_width()
        .radius(THEME.radii.xl)
        .gradient(Gradient::diagonal(INDIGO, VIOLET))
        .shadow(Shadow::new(0.0, 12.0, 28.0, with_alpha(INDIGO, 0.35)))
        .clip()
        .id("balance-card")
    }

    fn quick_actions(&self) -> Container<Message> {
        let action =
            |index: u8, glyph: &'static str, label: &'static str, tint: Color, ink: Color| {
                Pressable::new(
                    col![
                        Icon::new(glyph)
                            .size(56.0)
                            .radius(18.0)
                            .background(tint)
                            .color(ink),
                        text(label).caption().medium(),
                    ]
                    .gap(8.0)
                    .cross_align(CrossAlign::Center),
                )
                .padding(4.0)
                .radius(20.0)
                .on_press(Message::QuickAction(index))
            };
        row![
            action(0, "»", self.tr("Tuma", "Send"), SKY, INDIGO),
            action(1, "«", self.tr("Pokea", "Receive"), MINT, hex(0x12925A)),
            action(2, "+", self.tr("Ongeza", "Top up"), AMBER, AMBER_INK),
            action(3, "…", self.tr("Zaidi", "More"), ROSE, hex(0xD92D2D)),
        ]
        .space_between()
        .fill_width()
        .padding_xy(4.0, 0.0)
    }

    fn transactions(&self) -> Container<Message> {
        let mut card = Column::new()
            .fill_width()
            .radius(THEME.radii.xl)
            .background(ThemeColor::Surface)
            .shadow(elevation::LOW)
            .clip();
        for (index, transaction) in TRANSACTIONS.iter().enumerate() {
            if index > 0 {
                // The line starts where the text does: 16 + 40 + 12.
                card = card.push(Divider::new().inset(68.0));
            }
            card = card.push(self.transaction_row(index, transaction));
        }

        col![
            row![
                text(self.tr("Miamala ya karibuni", "Recent transactions")).headline(),
                Spacer::new(),
                Pressable::new(
                    text(self.tr("Ona zote", "See all"))
                        .label()
                        .color(ThemeColor::Primary)
                )
                .padding_ltrb(8.0, 6.0, 4.0, 6.0)
                .radius(8.0)
                .on_press(Message::Nothing),
            ]
            .cross_align(CrossAlign::Center)
            .fill_width(),
            card,
        ]
        .gap(10.0)
        .fill_width()
    }

    fn transaction_row(&self, index: usize, transaction: &Transaction) -> Container<Message> {
        let incoming = transaction.amount > 0;
        let amount = format!(
            "{} TSh {}",
            if incoming { "+" } else { "−" },
            grouped(transaction.amount.unsigned_abs())
        );
        let (tint, ink) = if incoming {
            (MINT, hex(0x12925A))
        } else {
            (SKY, INDIGO)
        };
        let mut row = row![
            Avatar::new(transaction.name).background(tint).color(ink),
            // The middle column fills, so it is what gives way (and wraps)
            // when the amount is long.
            col![
                text(transaction.name).semibold().max_lines(1).fill_width(),
                text(self.tr(transaction.swahili, transaction.english))
                    .caption()
                    .muted()
                    .max_lines(1)
                    .fill_width(),
            ]
            .gap(2.0)
            .fill_width(),
            text(amount).label().color(if incoming {
                ColorRef::from(ThemeColor::Success)
            } else {
                ColorRef::from(ThemeColor::Text)
            }),
        ]
        .gap(12.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding_xy(16.0, 12.0)
        .on_press(Message::OpenTransaction(index))
        // Rows keep their identity if the list changes under a finger.
        .id(("transaction", index));
        if self.selected_transaction == Some(index) {
            row = row.background(ThemeColor::PrimaryContainer);
        }
        row
    }

    /// The bottom bar: its background runs under the gesture bar, its tabs
    /// stay above it.
    fn tab_bar(&self) -> Container<Message> {
        let tab = |screen: Screen, label: &'static str, icon: Container<Message>| {
            let selected = self.screen == screen;
            Pressable::new(
                col![
                    Container::new(icon)
                        .size(56.0, 30.0)
                        .center()
                        .radius(15.0)
                        .background(if selected {
                            THEME.colors.primary_container
                        } else {
                            with_alpha(Color::WHITE, 0.0)
                        }),
                    text(label).caption().semibold().color(if selected {
                        ThemeColor::Primary
                    } else {
                        ThemeColor::TextMuted
                    }),
                ]
                .gap(4.0)
                .cross_align(CrossAlign::Center),
            )
            .center()
            .fill_width()
            .padding_xy(0.0, 8.0)
            .on_press(Message::Open(screen))
            .id(("tab", label))
        };
        let tint = |screen: Screen| {
            if self.screen == screen {
                THEME.colors.primary
            } else {
                THEME.colors.text_muted
            }
        };

        col![
            Divider::new(),
            row![
                tab(
                    Screen::Home,
                    self.tr("Nyumbani", "Home"),
                    home_icon(tint(Screen::Home))
                ),
                tab(
                    Screen::Settings,
                    self.tr("Mipangilio", "Settings"),
                    menu_icon(tint(Screen::Settings))
                ),
            ]
            .fill_width(),
        ]
        .fill_width()
        .background(ThemeColor::Surface)
        .safe_area(SafeEdges::BOTTOM)
        .id("tab-bar")
    }
}

/// A house-ish outline, from two boxes. Stands in for an icon image.
fn home_icon(color: Color) -> Container<Message> {
    col![
        Container::empty()
            .size(10.0, 2.0)
            .radius(1.0)
            .background(color),
        Container::empty()
            .size(18.0, 12.0)
            .radius(4.0)
            .border(2.0, color),
    ]
    .gap(2.0)
    .cross_align(CrossAlign::Center)
}

/// Three sliders-like bars. Stands in for an icon image.
fn menu_icon(color: Color) -> Container<Message> {
    col![
        Container::empty()
            .size(18.0, 2.0)
            .radius(1.0)
            .background(color),
        Container::empty()
            .size(12.0, 2.0)
            .radius(1.0)
            .background(color),
        Container::empty()
            .size(18.0, 2.0)
            .radius(1.0)
            .background(color),
    ]
    .gap(4.0)
    .cross_align(CrossAlign::Start)
}

// -------------------------------------------------------------- settings

/// What sits at the trailing end of a settings row.
enum Trailing {
    /// A value and a chevron.
    Value(&'static str),
    /// An on/off switch.
    Switch(bool),
}

impl Showcase {
    fn settings(&self) -> Element<Message> {
        col![
            row![text(self.tr("Mipangilio", "Settings")).title()]
                .fill_width()
                .padding_ltrb(MARGIN, 12.0, MARGIN, 8.0)
                .safe_area(SafeEdges::TOP),
            ScrollView::new(
                col![
                    self.profile_card(),
                    section(
                        self.tr("AKAUNTI", "ACCOUNT"),
                        vec![
                            setting_row(
                                "P",
                                SKY,
                                INDIGO,
                                self.tr("Taarifa binafsi", "Personal details"),
                                Trailing::Value(""),
                                Message::Nothing,
                            ),
                            setting_row(
                                "#",
                                MINT,
                                hex(0x12925A),
                                self.tr("Usalama na PIN", "Security and PIN"),
                                Trailing::Value(""),
                                Message::Nothing,
                            ),
                            setting_row(
                                "L",
                                AMBER,
                                AMBER_INK,
                                self.tr("Vikomo vya miamala", "Transaction limits"),
                                Trailing::Value("TSh 5M"),
                                Message::Nothing,
                            ),
                        ],
                    ),
                    section(
                        self.tr("MAPENDELEO", "PREFERENCES"),
                        vec![
                            setting_row(
                                "A",
                                SKY,
                                INDIGO,
                                self.tr("Lugha", "Language"),
                                Trailing::Value(self.tr("Kiswahili", "English")),
                                Message::ToggleLanguage,
                            )
                            .id("language-row"),
                            setting_row(
                                "!",
                                ROSE,
                                hex(0xD92D2D),
                                self.tr("Arifa", "Notifications"),
                                Trailing::Switch(self.notifications),
                                Message::ToggleNotifications,
                            )
                            .id("notifications-row"),
                            setting_row(
                                "B",
                                MINT,
                                hex(0x12925A),
                                self.tr("Alama ya kidole", "Fingerprint unlock"),
                                Trailing::Switch(self.biometrics),
                                Message::ToggleBiometrics,
                            )
                            .id("biometrics-row"),
                        ],
                    ),
                    section(
                        self.tr("MSAADA", "SUPPORT"),
                        vec![
                            setting_row(
                                "?",
                                AMBER,
                                AMBER_INK,
                                self.tr("Kituo cha msaada", "Help centre"),
                                Trailing::Value(""),
                                Message::Nothing,
                            ),
                            setting_row(
                                "§",
                                SKY,
                                INDIGO,
                                self.tr("Vigezo na masharti", "Terms and conditions"),
                                Trailing::Value(""),
                                Message::Nothing,
                            ),
                        ],
                    ),
                    Button::new(self.tr("Ondoka kwenye akaunti", "Sign out"))
                        .destructive()
                        .on_press(Message::Open(Screen::Login))
                        .fill_width()
                        .id("sign-out"),
                    text(self.tr("Toleo 1.0.0 (42)", "Version 1.0.0 (42)"))
                        .caption()
                        .muted()
                        .centered()
                        .fill_width(),
                ]
                .gap(22.0)
                .padding_ltrb(MARGIN, 8.0, MARGIN, 28.0),
            )
            .id("settings-scroll"),
            self.tab_bar(),
        ]
        .id("settings")
        .into()
    }

    fn profile_card(&self) -> Container<Message> {
        row![
            Avatar::new("Amina Juma")
                .image_maybe(self.portrait)
                .size(56.0),
            col![
                text("Amina Juma").headline(),
                text("+255 712 345 678").muted(),
            ]
            .gap(2.0)
            .fill_width(),
            Pressable::new(
                text(self.tr("Hariri", "Edit"))
                    .label()
                    .color(ThemeColor::Primary)
            )
            .padding_xy(14.0, 8.0)
            .radius(THEME.radii.full)
            .background(ThemeColor::PrimaryContainer)
            .on_press(Message::Nothing),
        ]
        .gap(14.0)
        .cross_align(CrossAlign::Center)
        .fill_width()
        .padding(16.0)
        .radius(THEME.radii.xl)
        .background(ThemeColor::Surface)
        .shadow(elevation::LOW)
    }
}

/// A small-caps section header over a card of rows separated by dividers.
fn section(title: &'static str, rows: Vec<Container<Message>>) -> Container<Message> {
    let mut card = Column::new()
        .fill_width()
        .radius(THEME.radii.lg)
        .background(ThemeColor::Surface)
        .shadow(elevation::LOW)
        .clip();
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            card = card.push(Divider::new().inset(64.0));
        }
        card = card.push(row);
    }
    col![
        Container::new(text(title).caption().semibold().muted()).padding_xy(6.0, 0.0),
        card,
    ]
    .gap(8.0)
    .fill_width()
}

/// Icon tile, label, then a value with a chevron or a switch.
fn setting_row(
    glyph: &'static str,
    tint: Color,
    ink: Color,
    label: &'static str,
    trailing: Trailing,
    message: Message,
) -> Container<Message> {
    let trailing: Element<Message> = match trailing {
        Trailing::Value(value) => row![
            text(value).muted(),
            text("›").size(20.0).line_height(1.0).muted(),
        ]
        .gap(8.0)
        .cross_align(CrossAlign::Center)
        .into(),
        Trailing::Switch(on) => switch(on).into(),
    };
    row![
        Icon::new(glyph)
            .size(36.0)
            .radius(10.0)
            .background(tint)
            .color(ink),
        text(label).fill_width(),
        trailing,
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .min_height(56.0)
    .padding_xy(16.0, 8.0)
    .on_press(message)
}

/// An on/off switch: a pill with a knob pinned to one side. It is purely a
/// picture of the state; the row around it handles the tap.
fn switch(on: bool) -> Container<Message> {
    Container::new(
        Container::empty()
            .size(22.0, 22.0)
            .radius(11.0)
            .background(Color::WHITE)
            .shadow(elevation::LOW),
    )
    .size(46.0, 28.0)
    .padding(3.0)
    .radius(14.0)
    .content_align(if on {
        Alignment::CENTER_RIGHT
    } else {
        Alignment::CENTER_LEFT
    })
    .background(if on {
        THEME.colors.primary
    } else {
        hex(0xC9CED8)
    })
}

// ----------------------------------------------------------------- login

impl Showcase {
    fn login(&self) -> Element<Message> {
        SafeArea::new(
            col![
                Spacer::fixed(28.0),
                // Logo: a gradient tile with a letter.
                Container::new(text("M").size(34.0).bold().color(Color::WHITE))
                    .size(72.0, 72.0)
                    .center()
                    .radius(22.0)
                    .gradient(Gradient::diagonal(INDIGO, VIOLET))
                    .shadow(Shadow::new(0.0, 10.0, 24.0, with_alpha(INDIGO, 0.35)))
                    .align_self(CrossAlign::Center),
                Spacer::fixed(24.0),
                text(self.tr("Karibu tena", "Welcome back"))
                    .title()
                    .centered()
                    .fill_width(),
                Spacer::fixed(6.0),
                text(self.tr(
                    "Ingia ili kuendelea na pochi yako",
                    "Sign in to continue to your wallet"
                ))
                .muted()
                .centered()
                .fill_width(),
                Spacer::fixed(4.0),
                // Arabic is shaped and laid out right to left by the text
                // engine. Filling the width gives it a box to align in.
                text("مرحباً بعودتك إلى محفظتك")
                    .muted()
                    .centered()
                    .fill_width()
                    .id("arabic"),
                Spacer::fixed(28.0),
                field(
                    self.tr("Namba ya simu", "Phone number"),
                    row![
                        text("+255").semibold(),
                        Divider::vertical(),
                        text("712 345 678").fill_width(),
                    ]
                    .gap(12.0)
                    .cross_align(CrossAlign::Center)
                    .fill(),
                ),
                Spacer::fixed(16.0),
                field(
                    "PIN",
                    row![
                        text("•  •  •  •").size(18.0).fill_width(),
                        text(self.tr("Onyesha", "Show"))
                            .label()
                            .color(ThemeColor::Primary),
                    ]
                    .cross_align(CrossAlign::Center)
                    .fill(),
                ),
                Spacer::fixed(4.0),
                Pressable::new(
                    text(self.tr("Umesahau PIN?", "Forgot PIN?"))
                        .label()
                        .color(ThemeColor::Primary)
                )
                .padding_xy(4.0, 12.0)
                .radius(8.0)
                .on_press(Message::Nothing)
                .align_self(CrossAlign::End),
                Spacer::fixed(8.0),
                Button::new(self.tr("Ingia", "Sign in"))
                    .on_press(Message::Open(Screen::Home))
                    .fill_width()
                    .height(52.0)
                    .radius(14.0)
                    .id("sign-in"),
                Spacer::fixed(20.0),
                row![
                    Divider::new(),
                    text(self.tr("au", "or")).caption().muted(),
                    Divider::new(),
                ]
                .gap(12.0)
                .cross_align(CrossAlign::Center)
                .fill_width(),
                Spacer::fixed(20.0),
                Button::new(self.tr("Fungua akaunti mpya", "Create a new account"))
                    .outlined()
                    .on_press(Message::Nothing)
                    .fill_width()
                    .height(52.0)
                    .radius(14.0),
                Spacer::new(),
                text(self.tr(
                    "Kwa kuendelea unakubali Vigezo na Masharti pamoja na Sera ya Faragha.",
                    "By continuing you agree to the Terms and Conditions and the Privacy Policy."
                ))
                .caption()
                .muted()
                .centered()
                .fill_width(),
                Spacer::fixed(12.0),
                Pressable::new(
                    text(self.tr("English", "Kiswahili"))
                        .label()
                        .color(ThemeColor::Primary)
                )
                .padding_xy(12.0, 8.0)
                .radius(8.0)
                .on_press(Message::ToggleLanguage)
                .align_self(CrossAlign::Center)
                .id("login-language"),
            ]
            .fill()
            .padding_ltrb(24.0, 0.0, 24.0, 12.0),
        )
        .background(ThemeColor::Surface)
        .id("login")
        .into()
    }
}

/// A labelled input field look-alike. There is no text input yet, so this
/// only draws what a filled-in field looks like.
fn field(label: &'static str, content: Container<Message>) -> Container<Message> {
    col![
        text(label).caption().semibold().muted(),
        Container::new(content)
            .fill_width()
            .height(54.0)
            .padding_xy(16.0, 0.0)
            .radius(14.0)
            .background(ThemeColor::Background)
            .border(1.0, ThemeColor::Divider),
    ]
    .gap(6.0)
    .fill_width()
}

morningstar::app!(Ui::new(Showcase::default()));
