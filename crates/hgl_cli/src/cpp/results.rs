//! What an executable of tests recorded of each test it ran: the results
//! file the runtime library's test host writes (`hglrt/host.h`).
//!
//! Each line is `start N`, `passed N`, `failed N LINE TEXT` or
//! `unloaded N TEXT`, where `N` is the test's position among those the
//! executable was asked to run. The results are a file, and not what the
//! executable prints, so that nothing a program or hgraph prints can be
//! taken for an outcome.

/// What a results file says of one test of the plan.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Recorded {
    /// True once a `start` line names it.
    pub(super) started: bool,
    /// What the last line that names it with an outcome says, if one
    /// does.
    pub(super) said: Option<Said>,
}

/// An outcome a results file gives a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Said {
    /// `passed N`: it ran to its end.
    Passed,
    /// `failed N LINE TEXT`: it failed in the statement that starts at
    /// or before `line` of the file its test is written in; `None`
    /// where the line is not a number.
    Failed {
        /// The line, counted from 1; 0 where the host did not know.
        line: Option<usize>,
        /// What the host said, with the protocol's escapes undone.
        message: String,
    },
    /// `unloaded N TEXT`: it could not be found or loaded, with why.
    Unloaded(String),
}

/// What `recorded`, the text of a results file, says of each of the
/// `count` tests of its plan, in the plan's order. A line that names
/// no test of the plan, or says nothing known, is passed over, and of
/// the lines that name a test with an outcome the last one counts.
pub(super) fn read_results(recorded: &str, count: usize) -> Vec<Recorded> {
    let mut read = vec![Recorded::default(); count];
    for line in recorded.lines() {
        let mut parts = line.splitn(3, ' ');
        let (Some(what), Some(number)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some(number) = number.parse::<usize>().ok().filter(|&n| n < count) else {
            continue;
        };
        let rest = parts.next().unwrap_or("");
        match what {
            "start" => read[number].started = true,
            "passed" => read[number].said = Some(Said::Passed),
            "failed" => {
                let (line, text) = rest.split_once(' ').unwrap_or((rest, ""));
                read[number].said = Some(Said::Failed {
                    line: line.parse::<usize>().ok(),
                    message: unescaped(text),
                });
            }
            "unloaded" => read[number].said = Some(Said::Unloaded(unescaped(rest))),
            _ => {}
        }
    }
    read
}

/// The text the host wrote with `\\`, `\n` and `\r` for a backslash,
/// a line feed and a carriage return.
fn unescaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Recorded, Said, read_results, unescaped};

    #[test]
    fn the_hosts_text_reads_back() {
        assert_eq!(unescaped(r"a\\b\nc\rd"), "a\\b\nc\rd");
        // A backslash the host would not write is kept.
        assert_eq!(unescaped(r"end\"), "end\\");
    }

    /// Each test's last outcome counts; a line naming no test of the
    /// plan is passed over.
    #[test]
    fn a_results_file_reads_back() {
        let recorded = "start 0\npassed 0\nstart 1\nfailed 1 12 a\\nb\nstart 2\nunloaded 9 x\n";
        assert_eq!(
            read_results(recorded, 3),
            [
                Recorded {
                    started: true,
                    said: Some(Said::Passed),
                },
                Recorded {
                    started: true,
                    said: Some(Said::Failed {
                        line: Some(12),
                        message: "a\nb".to_string(),
                    }),
                },
                Recorded {
                    started: true,
                    said: None,
                },
            ]
        );
    }
}
