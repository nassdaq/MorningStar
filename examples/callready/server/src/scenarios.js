// The four practice calls and the contract's enumerations.
//
// Titles, caller names, goals and opening lines are the same as the app's
// own script (`examples/callready/src/script.rs`), so a call started here
// begins the way the designed call does. Everything after the opening line
// is played by the model from `brief`.

export const ACCENTS = ['us', 'uk', 'australian', 'indian'];
export const DIFFICULTIES = ['calm', 'standard', 'tough'];
export const MINUTES = [2, 4, 6];
export const PLANS = ['free', 'premium'];

export const SCENARIOS = {
  missing_delivery: {
    title: 'Missing delivery',
    caller: 'Mark',
    gender: 'male',
    goal: 'Calm the caller and solve the problem',
    opening: 'Hi, my parcel was due last Monday.',
    tips: ['Offer to help', 'Apologise for the delay'],
    brief:
      'You ordered a parcel that was due last Monday. It has now been two weeks since you ordered and nothing has ' +
      'arrived, and nobody has told you anything. Your order number is 48213 (give it only when asked). You need the ' +
      'parcel by Friday. You are satisfied once the agent has acknowledged the delay, found the parcel and given you ' +
      'a specific delivery day or time.',
  },
  angry_customer: {
    title: 'Angry customer',
    caller: 'Dana',
    gender: 'female',
    goal: 'Let the caller be heard, then fix it',
    opening: 'This is the third time I’ve called today!',
    tips: ['Take ownership', 'Ask what happened'],
    brief:
      'This is your third call today. The company’s app keeps logging you out; every time, it says ' +
      '“session expired” (mention that message when asked what you see). You were transferred twice ' +
      'already and you have wasted your whole morning. You are satisfied once the agent has listened, taken ' +
      'ownership, fixed it or given a concrete fix, and promised a follow-up. Being told to reinstall or being ' +
      'transferred again makes you angrier.',
  },
  refund_request: {
    title: 'Refund request',
    caller: 'Priya',
    gender: 'female',
    goal: 'Refund the order and keep the customer',
    opening: 'I’d like a refund for these headphones, please.',
    tips: ['Ask what went wrong', 'Offer to help'],
    brief:
      'You bought wireless headphones ten days ago; the left side stopped working after two days. You want your ' +
      'money back, not a replacement. Your order number is 77190 (give it only when asked). You want to know ' +
      'whether you get the refund, how long it takes, and what to do with the broken pair. You are polite at first ' +
      'but become annoyed if the agent asks you to prove the purchase before listening, or suggests you broke them.',
  },
  billing_problem: {
    title: 'Billing problem',
    caller: 'Tom',
    gender: 'male',
    goal: 'Explain the charge and put it right',
    opening: 'You charged me twice for the same bill!',
    tips: ['Apologise and check', 'Ask for the details'],
    brief:
      'Your card shows two payments of $42 on the 3rd for the same monthly bill. Your account number is 30517 ' +
      '(give it only when asked). You want to know why it happened, when the money comes back, and how you can be ' +
      'sure it will not happen again. Jargon, vague timelines or blaming your bank irritate you; a plain ' +
      'explanation, a number of days and a fact about what was changed satisfy you.',
  },
};

export const SCENARIO_IDS = Object.keys(SCENARIOS);

/** How many learner replies a call may hold before the caller must close. */
export const MAX_LEARNER_TURNS = { 2: 8, 4: 14, 6: 20 };

/** What an ended call with no learner turn says in place of feedback. */
export const UNSCORED = {
  did_well: 'The call ended before you spoke.',
  try_next: 'Answer the caller’s first line to get a score.',
};
