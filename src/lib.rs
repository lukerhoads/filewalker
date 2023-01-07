use derive_builder::Builder;
use once_cell::sync::Lazy;
use rev_buf_reader::RevBufReader;
use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom, self},
    process::{Command, Stdio},
    vec::IntoIter,
};
use thiserror::Error;

// Position stores the cursor location as a byte offset
#[derive(Debug, Clone, Copy)]
pub enum Position {
    Start,
    Middle(usize),
    End,
}

impl Default for Position {
    fn default() -> Self {
        Position::Start
    }
}

impl From<usize> for Position {
    fn from(value: usize) -> Self {
        Position::Middle(value)
    }
}

impl From<&str> for Position {
    fn from(value: &str) -> Self {
        Position::from(value.to_string())
    }
}

impl From<String> for Position {
    fn from(value: String) -> Self {
        if let Ok(num) = value.parse::<usize>() {
            return Position::Middle(num);
        } else if value == "end" {
            return Position::End;
        }

        Position::default()
    }
}

impl From<Option<String>> for Position {
    fn from(value: Option<String>) -> Self {
        if let Some(pos) = value {
            return pos.into();
        }

        Position::default()
    }
}

// Direction indicates whether to parse the file moving up or down
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Forward,
    Backward,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Forward
    }
}

impl From<&str> for Direction {
    fn from(value: &str) -> Self {
        Direction::from(value.to_string())
    }
}

impl From<String> for Direction {
    fn from(value: String) -> Self {
        if value == "backward" {
            return Direction::Backward;
        }

        Direction::default()
    }
}

impl From<Option<String>> for Direction {
    fn from(value: Option<String>) -> Self {
        if let Some(dir) = value {
            return Direction::from(dir);
        }

        Direction::default()
    }
}

#[derive(Builder)]
pub struct Opener {
    path: String,
    #[builder(setter(into, strip_option), default)]
    position: Option<Position>,
    #[builder(setter(into, strip_option), default)]
    direction: Option<Direction>,
    #[builder(setter(into, strip_option), default)]
    max_position: Option<Position>,
}

impl Opener {
    pub fn open(&self) -> Result<IntoIter<String>, Error> {
        open_file(
            &self.path,
            self.position.unwrap_or_default(),
            self.direction.unwrap_or_default(),
            self.max_position,
        )
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("File error.")]
    File(#[from] io::Error),

    #[error("Cannot go {dir:?} from the {pos:?} position.")]
    InvalidDirection {
        pos: String,
        dir: String
    },

    #[error("Cannot have a max line position {cmp:?} than the current line position when the direction is {dir:?}.")]
    MaxLinePosition {
        cmp: String,
        dir: String,
    }
}

// The main file of this crate. Opens a file and reads it according to your specification.
pub fn open_file<T: Into<String>, P: Into<Position>, D: Into<Direction>>(
    path: T,
    position: P,
    direction: D,
    max_position: Option<Position>,
) -> Result<IntoIter<String>, Error> {
    let path = path.into();
    let position = position.into();
    let direction = direction.into();

    let mut input = match File::open(path.as_str()) {
        Ok(v) => v,
        Err(e) => return Err(Error::File(e))
    };
        
    let buf = BufReader::new(&input);
    let total_lines = buf.lines().count();

    let position_number = match position {
        Position::Start => 1,
        Position::Middle(n) => n,
        Position::End => total_lines,
    };

    let max_position_number = if let Some(pos) = max_position {
        let pos = pos.into();
        Some(match pos {
            Position::Start => 0,
            Position::Middle(n) => n,
            Position::End => total_lines,
        })
    } else {
        None
    };

    if matches!(direction, Direction::Backward) && matches!(position, Position::Start) {
        return Err(Error::InvalidDirection {
            pos: "start".to_string(),
            dir: "backwards".to_string()
        })
    } else if matches!(direction, Direction::Forward) && matches!(position, Position::End) {
        return Err(Error::InvalidDirection {
            pos: "end".to_string(),
            dir: "forwards".to_string()
        })
    } else if max_position_number.is_some() {
        if matches!(direction, Direction::Forward) && max_position_number.unwrap() < position_number
        {
            return Err(Error::MaxLinePosition { 
                cmp: "less".to_string(),
                dir: "forward".to_string()
            });
        } else if matches!(direction, Direction::Backward)
            && max_position_number.unwrap() > position_number
        {
            return Err(Error::MaxLinePosition { 
                cmp: "greater".to_string(),
                dir: "backward".to_string()
            });
        }
    }

    let new_line_pos = if let Position::Middle(num) = position {
        if matches!(direction, Direction::Backward) {
            Position::Middle(num + 1)
        } else {
            Position::Middle(num)
        }
    } else {
        position
    };

    if let Err(e) = input
        .seek(match position {
            Position::Start => SeekFrom::Start(0),
            Position::Middle(_) => {
                let byte_offset = compute_offset(&path, new_line_pos);
                SeekFrom::Start(byte_offset as u64)
            }
            Position::End => SeekFrom::End(0),
        }) {
        return Err(Error::File(e))
    }
        
    let mut offset_buf: Box<dyn BufRead + Send> = match direction {
        Direction::Forward => Box::new(BufReader::new(input)),
        Direction::Backward => Box::new(RevBufReader::new(input)),
    };

    let mut curr_line = match position {
        Position::Start => 1,
        Position::Middle(line) => line,
        Position::End => total_lines,
    };

    let mut lines = vec![];
    while curr_line > 0 && curr_line <= total_lines {
        if max_position_number.is_some() {
            let max_position_number = max_position_number.unwrap();
            if (curr_line > max_position_number && matches!(direction, Direction::Forward))
                || (curr_line < max_position_number && matches!(direction, Direction::Backward))
            {
                break;
            }
        }

        let mut line = String::new();
        offset_buf.as_mut().read_line(&mut line).unwrap();
        lines.push(line.replace("\n", ""));
        if curr_line <= total_lines && matches!(direction, Direction::Forward) {
            curr_line += 1;
        } else if curr_line > 0 && matches!(direction, Direction::Backward) {
            curr_line -= 1;
        } else {
            continue;
        }
    }

    Ok(lines.into_iter())
}

fn compute_offset(input_file: &str, position: Position) -> usize {
    match position {
        Position::Middle(line) => {
            let init_grep = Command::new("grep")
                .args(["-b", "-n", "", input_file])
                .stdout(Stdio::piped())
                .spawn()
                .expect("Failed to launch first grep command");
            let final_grep = Command::new("grep")
                .arg(format!("^{}:", line))
                .stdin(
                    init_grep
                        .stdout
                        .expect("Unable to get stdout from previous grep command."),
                )
                .output()
                .expect("Failed to launch second grep command");
            String::from_utf8_lossy(&final_grep.stdout)
                .into_owned()
                .split(":")
                .nth(1)
                .expect("Unable to access offset element of extraction result.")
                .parse()
                .expect("Unable to parse resulting position.")
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static RESULTS_1: Lazy<Vec<String>> = Lazy::new(|| {
        vec!["hello", "there", "whats", "up"]
            .iter()
            .map(|i| i.to_string())
            .collect()
    });
    const RESULTS_2: Lazy<Vec<String>> = Lazy::new(|| {
        vec!["am i clear now"]
            .iter()
            .map(|i| i.to_string())
            .collect()
    });

    #[test]
    fn test_open_file_forward() {
        for (idx, line) in open_file("./testfiles/1.txt", None, None, None)
            .unwrap()
            .enumerate()
        {
            assert_eq!(*RESULTS_1[idx], line);
        }
    }

    #[test]
    fn test_open_file_backward() {
        let mut results: Vec<String> = RESULTS_1.clone();
        results.reverse();
        for (idx, line) in open_file(
            "./testfiles/1.txt",
            Position::End,
            Direction::Backward,
            None,
        )
        .unwrap()
        .enumerate()
        {
            assert_eq!(results[idx], line);
        }
    }

    #[test]
    fn test_one_line_file() {
        let mut forward = vec![];
        for line in open_file("./testfiles/2.txt", None, None, None).unwrap() {
            forward.push(line);
        }

        let mut backward = vec![];
        for line in open_file(
            "./testfiles/2.txt",
            Position::End,
            Direction::Backward,
            Some(Position::End),
        )
        .unwrap()
        {
            backward.push(line);
        }

        let mut middle = vec![];
        for line in open_file(
            "./testfiles/2.txt",
            Position::Middle(1),
            Direction::Forward,
            None,
        )
        .unwrap()
        {
            middle.push(line);
        }

        assert_eq!(forward, backward);
        assert_eq!(backward, middle);
        assert_eq!(forward, *RESULTS_2);
    }

    #[test]
    fn test_empty_file() {
        let mut results = vec![];
        for line in open_file("./testfiles/3.txt", None, None, None).unwrap() {
            results.push(line);
        }

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_max_position() {
        let mut max_for = vec![];
        for line in open_file("./testfiles/1.txt", None, None, Some(Position::Middle(2))).unwrap() {
            max_for.push(line);
        }
    }

    #[test]
    fn test_string_args() {
        let mut results: Vec<String> = RESULTS_1.clone();
        results.reverse();
        for (idx, line) in open_file("./testfiles/1.txt", "end", "backward", None)
            .unwrap()
            .enumerate()
        {
            assert_eq!(results[idx], line);
        }
    }

    #[test]
    fn test_builder() {
        let opener = OpenerBuilder::default()
            .path("./testfiles/3.txt".to_string())
            .build()
            .unwrap()
            .open()
            .unwrap();

        assert_eq!(opener.len(), 0)
    }

    #[test]
    fn test_error_cases() {
        let opener = OpenerBuilder::default()
            .path("./testfiles/3.txt".to_string())
            .position("start")
            .direction("backward")
            .build()
            .unwrap()
            .open()
            .unwrap_err();
        assert_eq!("Cannot go \"backwards\" from the \"start\" position.", opener.to_string()); 
        let opener = OpenerBuilder::default()
            .path("./testfiles/3.txt".to_string())
            .position("end")
            .direction("forward")
            .build()
            .unwrap()
            .open()
            .unwrap_err();
        assert_eq!("Cannot go \"forwards\" from the \"end\" position.", opener.to_string());
        let opener = OpenerBuilder::default()
            .path("./testfiles/3.txt".to_string())
            .position("3")
            .direction("forward")
            .max_position("2")
            .build()
            .unwrap()
            .open()
            .unwrap_err();
        assert_eq!("Cannot have a max line position \"less\" than the current line position when the direction is \"forward\".", opener.to_string()); 
        let opener = OpenerBuilder::default()
            .path("./testfiles/3.txt".to_string())
            .position("2")
            .direction("backward")
            .max_position("3")
            .build()
            .unwrap()
            .open()
            .unwrap_err();
        assert_eq!("Cannot have a max line position \"greater\" than the current line position when the direction is \"backward\".", opener.to_string()); 
    }
}
