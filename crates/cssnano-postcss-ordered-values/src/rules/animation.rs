//! Port of `src/rules/animation.js`.
//!
//! `animation: [ none | <keyframes-name> ] || <time> || <single-timing-function>
//!  || <time> || <single-animation-iteration-count> || <single-animation-direction>
//!  || <single-animation-fill-mode> || <single-animation-play-state>`

use cssnano_utils::get_arguments;
use postcss_value_parser::parse::{Node, NodeKind};
use postcss_value_parser::unit::parse_unit;

use crate::helpers::add_space::add_space;
use crate::helpers::get_value::get_value;

fn is_function_timing(value: &str, kind: &NodeKind) -> bool {
    if !matches!(kind, NodeKind::Function) { return false; }
    matches!(value, "steps" | "cubic-bezier" | "frames")
}

fn is_keyword_timing(value: &str) -> bool {
    matches!(
        value,
        "ease" | "ease-in" | "ease-in-out" | "ease-out" | "linear" | "step-end" | "step-start"
    )
}

fn is_timing_function(value: &str, kind: &NodeKind) -> bool {
    is_function_timing(value, kind) || is_keyword_timing(value)
}

fn is_direction(value: &str) -> bool {
    matches!(value, "normal" | "reverse" | "alternate" | "alternate-reverse")
}

fn is_fill_mode(value: &str) -> bool {
    matches!(value, "none" | "forwards" | "backwards" | "both")
}

fn is_play_state(value: &str) -> bool {
    matches!(value, "running" | "paused")
}

fn is_time(value: &str) -> bool {
    match parse_unit(value) {
        Some(q) => matches!(q.unit.as_str(), "ms" | "s"),
        None => false,
    }
}

fn is_iteration_count(value: &str) -> bool {
    if value == "infinite" { return true; }
    match parse_unit(value) {
        Some(q) => q.unit.is_empty(),
        None => false,
    }
}

#[derive(Debug, Clone, Copy)]
enum Bucket {
    Duration,
    TimingFunction,
    Delay,
    IterationCount,
    Direction,
    FillMode,
    PlayState,
}

fn delegate(b: Bucket, value: &str, kind: &NodeKind) -> bool {
    match b {
        Bucket::Duration => is_time(value),
        Bucket::TimingFunction => is_timing_function(value, kind),
        Bucket::Delay => is_time(value),
        Bucket::IterationCount => is_iteration_count(value),
        Bucket::Direction => is_direction(value),
        Bucket::FillMode => is_fill_mode(value),
        Bucket::PlayState => is_play_state(value),
    }
}

const STATE_CONDITIONS: &[Bucket] = &[
    Bucket::Duration,
    Bucket::TimingFunction,
    Bucket::Delay,
    Bucket::IterationCount,
    Bucket::Direction,
    Bucket::FillMode,
    Bucket::PlayState,
];

#[derive(Default, Debug)]
struct State {
    name: Vec<Node>,
    duration: Vec<Node>,
    timing_function: Vec<Node>,
    delay: Vec<Node>,
    iteration_count: Vec<Node>,
    direction: Vec<Node>,
    fill_mode: Vec<Node>,
    play_state: Vec<Node>,
}

impl State {
    fn bucket_mut(&mut self, b: Bucket) -> &mut Vec<Node> {
        match b {
            Bucket::Duration => &mut self.duration,
            Bucket::TimingFunction => &mut self.timing_function,
            Bucket::Delay => &mut self.delay,
            Bucket::IterationCount => &mut self.iteration_count,
            Bucket::Direction => &mut self.direction,
            Bucket::FillMode => &mut self.fill_mode,
            Bucket::PlayState => &mut self.play_state,
        }
    }
}

fn normalize(args: Vec<Vec<Node>>) -> Vec<Vec<Node>> {
    let mut list: Vec<Vec<Node>> = Vec::with_capacity(args.len());

    for arg in args {
        let mut state = State::default();

        for node in arg {
            if node.kind == NodeKind::Space { continue; }

            let lowered = node.value.to_lowercase();
            let mut matched = false;
            for bucket in STATE_CONDITIONS {
                if delegate(*bucket, &lowered, &node.kind) && state.bucket_mut(*bucket).is_empty() {
                    let slot = state.bucket_mut(*bucket);
                    slot.push(node.clone());
                    slot.push(add_space());
                    matched = true;
                    break;
                }
            }
            if !matched {
                state.name.push(node);
                state.name.push(add_space());
            }
        }

        let mut combined = Vec::new();
        combined.extend(state.name);
        combined.extend(state.duration);
        combined.extend(state.timing_function);
        combined.extend(state.delay);
        combined.extend(state.iteration_count);
        combined.extend(state.direction);
        combined.extend(state.fill_mode);
        combined.extend(state.play_state);
        list.push(combined);
    }

    list
}

pub fn normalize_animation(parsed_nodes: Vec<Node>) -> String {
    let args = get_arguments(&parsed_nodes, |n: &Node| {
        n.kind == NodeKind::Div && n.value == ","
    });
    let values = normalize(args);
    get_value(values)
}
