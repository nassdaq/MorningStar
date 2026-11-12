//! The practice calls: four scenarios, each a short scripted conversation.
//!
//! A call is a list of [`Exchange`]s. In each, the customer says a line and
//! the learner answers by picking one of two tips; the tip's words complete
//! the learner's sentence. One tip is the good answer and one is weaker.
//! Nothing here is generated or fetched: the calls are written out, run on
//! the phone and always play the same way.

/// The five skills a call is scored on, in the order the report lists
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Skill {
    /// Saying it plainly.
    Clarity,
    /// Acknowledging how the caller feels.
    Empathy,
    /// Asking and hearing before answering.
    Listening,
    /// Getting the thing fixed.
    ProblemSolving,
    /// Language and manner.
    Professional,
}

impl Skill {
    /// Every skill, in the report's order.
    pub const ALL: [Self; 5] = [
        Self::Clarity,
        Self::Empathy,
        Self::Listening,
        Self::ProblemSolving,
        Self::Professional,
    ];

    /// The name the report shows.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Clarity => "Clarity",
            Self::Empathy => "Empathy",
            Self::Listening => "Listening",
            Self::ProblemSolving => "Problem solving",
            Self::Professional => "Professional language",
        }
    }
}

/// One way to answer the customer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    /// The tip on the chip: the approach, not the words.
    pub tip: &'static str,
    /// The words that complete the learner's sentence.
    pub words: &'static str,
    /// Whether this is the answer a good agent gives.
    pub good: bool,
}

impl Choice {
    const fn good(tip: &'static str, words: &'static str) -> Self {
        Self {
            tip,
            words,
            good: true,
        }
    }

    const fn weak(tip: &'static str, words: &'static str) -> Self {
        Self {
            tip,
            words,
            good: false,
        }
    }
}

/// The customer's line and the learner's answer to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exchange {
    /// The first words of the customer's line, shown in the accent colour.
    pub lead: &'static str,
    /// The rest of the customer's line, starting with its space.
    pub rest: &'static str,
    /// The start of the learner's sentence; a tip completes it.
    pub reply: &'static str,
    /// The two tips, in the order the chips show them.
    pub options: [Choice; 2],
    /// The skill this exchange practises most.
    pub skill: Skill,
    /// What the report says when the good answer was given.
    pub praise: &'static str,
    /// What the report says when the weaker one was.
    pub advice: &'static str,
}

impl Exchange {
    /// The customer's whole line.
    pub fn customer(&self) -> String {
        format!("{}{}", self.lead, self.rest)
    }

    /// The learner's whole sentence with `option` chosen.
    pub fn spoken(&self, option: usize) -> String {
        format!("{} {}", self.reply, self.options[option].words)
    }

    /// The position of the good answer among the chips.
    pub fn good_option(&self) -> usize {
        usize::from(!self.options[0].good)
    }
}

/// A kind of call to practise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Scenario {
    /// Today's practice call on the home screen, and the call the design
    /// shows in progress.
    #[default]
    MissingDelivery,
    /// Scenario card 1.
    AngryCustomer,
    /// Scenario card 2.
    RefundRequest,
    /// Scenario card 3.
    BillingProblem,
}

impl Scenario {
    /// Every scenario: today's call, then the three cards in their order.
    pub const ALL: [Self; 4] = [
        Self::MissingDelivery,
        Self::AngryCustomer,
        Self::RefundRequest,
        Self::BillingProblem,
    ];

    /// The three scenario cards of the home screen, in their order.
    pub const CARDS: [Self; 3] = [
        Self::AngryCustomer,
        Self::RefundRequest,
        Self::BillingProblem,
    ];

    /// The scenario's name.
    pub const fn title(self) -> &'static str {
        match self {
            Self::MissingDelivery => "Missing delivery",
            Self::AngryCustomer => "Angry customer",
            Self::RefundRequest => "Refund request",
            Self::BillingProblem => "Billing problem",
        }
    }

    /// The first name of the customer who calls.
    pub const fn caller(self) -> &'static str {
        match self {
            Self::MissingDelivery => "Mark",
            Self::AngryCustomer => "Dana",
            Self::RefundRequest => "Priya",
            Self::BillingProblem => "Tom",
        }
    }

    /// What the learner is to achieve: the heading of the call.
    pub const fn goal(self) -> &'static str {
        match self {
            Self::MissingDelivery => "Calm the caller and solve the problem",
            Self::AngryCustomer => "Let the caller be heard, then fix it",
            Self::RefundRequest => "Refund the order and keep the customer",
            Self::BillingProblem => "Explain the charge and put it right",
        }
    }

    /// The conversation.
    pub const fn script(self) -> &'static [Exchange] {
        match self {
            Self::MissingDelivery => &MISSING_DELIVERY,
            Self::AngryCustomer => &ANGRY_CUSTOMER,
            Self::RefundRequest => &REFUND_REQUEST,
            Self::BillingProblem => &BILLING_PROBLEM,
        }
    }
}

/// The second exchange is the moment the design shows: its customer line,
/// the start of the reply and both tips are the design's words.
const MISSING_DELIVERY: [Exchange; 4] = [
    Exchange {
        lead: "Hi,",
        rest: " my parcel was due last Monday",
        reply: "Thank you for calling. I\u{2019}m",
        options: [
            Choice::good("Offer to help", "here to help you find it."),
            Choice::weak("Put them on hold", "going to put you on hold."),
        ],
        skill: Skill::Professional,
        praise: "You opened by offering to help.",
        advice: "Offer to help before you put a caller on hold.",
    },
    Exchange {
        lead: "Two weeks",
        rest: " and still nothing. This is ridiculous!",
        reply: "I completely understand, and I\u{2019}m sorry for the",
        options: [
            Choice::good("Acknowledge the delay", "delay. Two weeks is too long."),
            Choice::weak("Ask for the order number", "trouble. Order number?"),
        ],
        skill: Skill::Empathy,
        praise: "You apologised early and kept a calm tone.",
        advice: "Acknowledge the delay before you ask for anything.",
    },
    Exchange {
        lead: "So where",
        rest: " is it? Nobody tells me anything.",
        reply: "Let me find out right now. Could I have your",
        options: [
            Choice::weak(
                "Explain how couriers work",
                "patience while I explain the courier process?",
            ),
            Choice::good("Ask for the order number", "order number, please?"),
        ],
        skill: Skill::ProblemSolving,
        praise: "You went straight to the order number.",
        advice: "Ask for the order number before explaining.",
    },
    Exchange {
        lead: "It\u{2019}s 48213.",
        rest: " I need it by Friday.",
        reply: "Thank you. It is at the local depot, and I\u{2019}ll",
        options: [
            Choice::good(
                "Promise a clear next step",
                "book delivery for tomorrow and text you the time.",
            ),
            Choice::weak("Say it will come soon", "make sure it gets to you soon."),
        ],
        skill: Skill::Clarity,
        praise: "You gave a clear next step with a day and a time.",
        advice: "Give a day and a time, not \u{201c}soon\u{201d}.",
    },
];

const ANGRY_CUSTOMER: [Exchange; 4] = [
    Exchange {
        lead: "This is the third time",
        rest: " I\u{2019}ve called today!",
        reply: "I\u{2019}m sorry you had to call again. I\u{2019}m going to",
        options: [
            Choice::weak("Transfer the call", "transfer you to another team."),
            Choice::good("Take ownership", "stay with you until this is fixed."),
        ],
        skill: Skill::Empathy,
        praise: "You took ownership instead of passing the call on.",
        advice: "Take ownership before you think of transferring.",
    },
    Exchange {
        lead: "Your app",
        rest: " keeps logging me out. It\u{2019}s useless!",
        reply: "That sounds really frustrating.",
        options: [
            Choice::good(
                "Ask what happens",
                "What do you see just before it logs you out?",
            ),
            Choice::weak("Defend the app", "It does work for most people, though."),
        ],
        skill: Skill::Listening,
        praise: "You asked what happens and let the caller explain.",
        advice: "Ask what the caller sees; don\u{2019}t defend the product.",
    },
    Exchange {
        lead: "It says",
        rest: " \u{201c}session expired\u{201d} every single time.",
        reply: "Thank you, that helps. I\u{2019}ll",
        options: [
            Choice::good(
                "Fix it and say how",
                "reset your session now so it stays signed in.",
            ),
            Choice::weak(
                "Tell them to reinstall",
                "need you to delete the app and reinstall it.",
            ),
        ],
        skill: Skill::ProblemSolving,
        praise: "You fixed it on the call and said what you did.",
        advice: "Fix what you can yourself before giving the caller a job.",
    },
    Exchange {
        lead: "Fine.",
        rest: " But I\u{2019}ve wasted my whole morning.",
        reply: "You\u{2019}re right, and I\u{2019}m sorry. I\u{2019}ve",
        options: [
            Choice::weak(
                "End the call quickly",
                "fixed it now, so is that everything?",
            ),
            Choice::good(
                "Close with a follow-up",
                "added a note, and I\u{2019}ll email you tomorrow to check.",
            ),
        ],
        skill: Skill::Professional,
        praise: "You closed with a follow-up, not just a goodbye.",
        advice: "End with a follow-up so the caller need not call again.",
    },
];

const REFUND_REQUEST: [Exchange; 4] = [
    Exchange {
        lead: "I\u{2019}d like a refund",
        rest: " for these headphones, please.",
        reply: "Of course, I can help with that. May I ask",
        options: [
            Choice::good("Ask what went wrong", "what went wrong with them?"),
            Choice::weak(
                "Ask for proof first",
                "if you can prove you bought them here?",
            ),
        ],
        skill: Skill::Listening,
        praise: "You asked what went wrong before anything else.",
        advice: "Ask what went wrong before you ask for proof.",
    },
    Exchange {
        lead: "The left side",
        rest: " stopped working after two days.",
        reply: "I\u{2019}m sorry about that. That is",
        options: [
            Choice::weak(
                "Suggest they broke it",
                "sometimes caused by rough handling.",
            ),
            Choice::good("Agree it is a fault", "clearly a fault, and it is covered."),
        ],
        skill: Skill::Empathy,
        praise: "You agreed it was a fault and did not blame the caller.",
        advice: "Never suggest the caller caused the fault.",
    },
    Exchange {
        lead: "So do I",
        rest: " get my money back or not?",
        reply: "Yes. I\u{2019}m refunding the full amount now, and it will",
        options: [
            Choice::good(
                "Give the timeline",
                "reach your card in three to five working days.",
            ),
            Choice::weak("Stay vague", "arrive at some point soon."),
        ],
        skill: Skill::Clarity,
        praise: "You gave the refund timeline in plain days.",
        advice: "Give the number of days, not \u{201c}soon\u{201d}.",
    },
    Exchange {
        lead: "And what",
        rest: " do I do with the broken pair?",
        reply: "I\u{2019}m emailing you a free returns label. You can",
        options: [
            Choice::weak(
                "Leave them to it",
                "look up the returns address on our website.",
            ),
            Choice::good(
                "Make it easy",
                "drop the parcel at any post office this week.",
            ),
        ],
        skill: Skill::Professional,
        praise: "You made the return easy and said exactly how.",
        advice: "Tell the caller exactly what to do with the return.",
    },
];

const BILLING_PROBLEM: [Exchange; 4] = [
    Exchange {
        lead: "You charged me twice",
        rest: " for the same bill!",
        reply: "I\u{2019}m sorry about that. Let me",
        options: [
            Choice::good("Check before promising", "look at your account right now."),
            Choice::weak(
                "Promise a refund unseen",
                "refund both payments straight away.",
            ),
        ],
        skill: Skill::ProblemSolving,
        praise: "You checked the account before promising anything.",
        advice: "Check the account before you promise a refund.",
    },
    Exchange {
        lead: "Two payments",
        rest: " of $42 on the 3rd. Look!",
        reply: "I can see both. The second one is",
        options: [
            Choice::weak("Use the jargon", "a pre-authorisation pending settlement."),
            Choice::good(
                "Explain it plainly",
                "a duplicate that our system took by mistake.",
            ),
        ],
        skill: Skill::Clarity,
        praise: "You explained the charge in plain words.",
        advice: "Say it in plain words; leave the jargon out.",
    },
    Exchange {
        lead: "So when",
        rest: " do I get it back?",
        reply: "I have reversed it now. You will see it in",
        options: [
            Choice::good(
                "Give the timeline",
                "three working days, and I\u{2019}ll text you when it lands.",
            ),
            Choice::weak("Blame the bank", "a while; that part is up to your bank."),
        ],
        skill: Skill::Professional,
        praise: "You gave a timeline and promised an update.",
        advice: "Give a timeline instead of pointing at the bank.",
    },
    Exchange {
        lead: "How do I know",
        rest: " it won\u{2019}t happen again?",
        reply: "That is a fair question. I have",
        options: [
            Choice::weak(
                "Brush it off",
                "never seen it happen twice, so don\u{2019}t worry.",
            ),
            Choice::good(
                "Reassure with a fact",
                "switched off the retry that caused it.",
            ),
        ],
        skill: Skill::Empathy,
        praise: "You reassured the caller with a fact.",
        advice: "Reassure with what you changed, not with \u{201c}don\u{2019}t worry\u{201d}.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_exchange_has_one_good_and_one_weaker_answer() {
        for scenario in Scenario::ALL {
            let script = scenario.script();
            assert!(script.len() >= 3, "{scenario:?}");
            for exchange in script {
                let good = exchange.options.iter().filter(|option| option.good).count();
                assert_eq!(good, 1, "{scenario:?}: {}", exchange.reply);
                assert!(exchange.rest.starts_with(' '), "{}", exchange.rest);
                assert!(exchange.options[exchange.good_option()].good);
            }
            // The good answer is not always the first chip.
            let first = script.iter().filter(|e| e.options[0].good).count();
            assert!(first > 0 && first < script.len(), "{scenario:?}");
        }
    }

    #[test]
    fn the_designed_moment_is_the_second_exchange_of_todays_call() {
        let exchange = Scenario::MissingDelivery.script()[1];
        assert_eq!(exchange.lead, "Two weeks");
        assert_eq!(exchange.rest, " and still nothing. This is ridiculous!");
        assert_eq!(
            exchange.reply,
            "I completely understand, and I\u{2019}m sorry for the"
        );
        assert_eq!(exchange.options[0].tip, "Acknowledge the delay");
        assert_eq!(exchange.options[1].tip, "Ask for the order number");
    }
}
