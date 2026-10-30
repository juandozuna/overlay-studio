//! El Dado question bank: strict UTF-8 CSV parsing into rows, all-or-nothing.
//! Columns (header required, case-insensitive): book, reference, question, a, b, c, d, correct.

const REQUIRED: [&str; 8] = ["book", "reference", "question", "a", "b", "c", "d", "correct"];
const MAX_ERRORS: usize = 10;
const SEMICOLON_HELP: &str = "This file uses semicolons. Save as CSV UTF-8 with commas.";

#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub book: String,
    pub reference: String,
    pub text: String,
    pub options: [String; 4],
    /// Host-only answer: the `correct` text, or "B · <option text>" when `correct` is a letter A-D.
    pub answer: String,
}
impl Question {
    pub fn has_options(&self) -> bool {
        self.options.iter().all(|o| !o.is_empty())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bank {
    pub rows: Vec<Question>,
}
impl Bank {
    /// Distinct books in file order.
    pub fn books(&self) -> Vec<String> {
        let mut books: Vec<String> = Vec::new();
        for row in &self.rows {
            if !row.book.is_empty() && !books.contains(&row.book) {
                books.push(row.book.clone());
            }
        }
        books
    }
    pub fn per_book(&self) -> Vec<(String, usize)> {
        self.books().into_iter().map(|b| { let n = self.rows.iter().filter(|r| r.book == b).count(); (b, n) }).collect()
    }
}

/// Strict UTF-8 decode that drops a leading BOM.
pub fn decode(bytes: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "The file is not UTF-8 text. Save as CSV UTF-8 with commas.".to_string())?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(text).to_string())
}

/// Parses the whole file or reports up to 10 problems with line numbers; never returns a partial bank.
pub fn parse(csv: &str) -> Result<Bank, String> {
    let csv = csv.strip_prefix('\u{feff}').unwrap_or(csv);
    let records = records(csv)?;
    let Some((_, header)) = records.first() else {
        return Err("The file is empty. Add a header row: book, reference, question, A, B, C, D, correct.".into());
    };
    let names: Vec<String> = header.iter().map(|h| h.trim().to_lowercase()).collect();
    let missing: Vec<&str> = REQUIRED.iter().copied().filter(|r| !names.iter().any(|n| n == r)).collect();
    if !missing.is_empty() {
        if header.iter().any(|h| h.contains(';')) {
            return Err(SEMICOLON_HELP.into());
        }
        return Err(format!("Missing columns: {}.", missing.join(", ")));
    }
    let at = |name: &str| names.iter().position(|n| n == name).unwrap_or(0);
    let [book, reference, question, a, b, c, d, correct] = REQUIRED.map(at);
    let mut rows = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut error_count = 0;
    for (line, fields) in &records[1..] {
        let mut problems = Vec::new();
        if fields.len() < names.len() {
            problems.push(format!("expected {} columns but found {}", names.len(), fields.len()));
        } else if fields[names.len()..].iter().any(|f| !f.trim().is_empty()) {
            problems.push(format!("expected {} columns but found {} (quote text that contains commas)", names.len(), fields.len()));
        } else {
            let get = |i: usize| fields[i].trim().to_string();
            let options = [get(a), get(b), get(c), get(d)];
            let text = get(question);
            let correct_raw = get(correct);
            if text.is_empty() {
                problems.push("question is empty".into());
            }
            let answer = match correct_letter(&correct_raw) {
                _ if correct_raw.is_empty() => {
                    problems.push("correct answer is empty".into());
                    String::new()
                }
                Some(i) if options[i].is_empty() => {
                    problems.push(format!("correct is {} but option {} is empty", correct_raw.to_uppercase(), correct_raw.to_uppercase()));
                    String::new()
                }
                Some(i) => format!("{} · {}", ["A", "B", "C", "D"][i], options[i]),
                None => correct_raw,
            };
            if problems.is_empty() {
                rows.push(Question { book: get(book), reference: get(reference), text, options, answer });
            }
        }
        for problem in problems {
            error_count += 1;
            if errors.len() < MAX_ERRORS {
                errors.push(format!("Line {line}: {problem}"));
            }
        }
    }
    if error_count > 0 {
        let more = if error_count > errors.len() { format!("\n…and {} more", error_count - errors.len()) } else { String::new() };
        return Err(format!("Could not import the questions:\n{}{more}", errors.join("\n")));
    }
    if rows.is_empty() {
        return Err("The file has a header but no questions.".into());
    }
    Ok(Bank { rows })
}

fn correct_letter(correct: &str) -> Option<usize> {
    let mut chars = correct.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => "abcd".find(c.to_ascii_lowercase()),
        _ => None,
    }
}

/// RFC 4180-style records with the line each starts on; blank lines are skipped.
/// A `;`-delimited file is refused instead of guessed.
fn records(text: &str) -> Result<Vec<(usize, Vec<String>)>, String> {
    let mut out: Vec<(usize, Vec<String>)> = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false; // inside quotes
    let mut was_quoted = false; // current field began with a quote
    let (mut line, mut start) = (1, 1);
    let mut chars = text.chars().peekable();
    let finish_record = |fields: &mut Vec<String>, field: &mut String, was_quoted: &mut bool, start: usize, out: &mut Vec<(usize, Vec<String>)>| {
        fields.push(std::mem::take(field));
        let blank = fields.len() == 1 && fields[0].trim().is_empty() && !*was_quoted;
        *was_quoted = false;
        let record = std::mem::take(fields);
        if !blank {
            out.push((start, record));
        }
    };
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                '\r' if chars.peek() == Some(&'\n') => {}
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() && !was_quoted => {
                quoted = true;
                was_quoted = true;
            }
            ',' => {
                fields.push(std::mem::take(&mut field));
                was_quoted = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' | '\n' => {
                finish_record(&mut fields, &mut field, &mut was_quoted, start, &mut out);
                line += 1;
                start = line;
            }
            _ => field.push(c),
        }
    }
    if quoted {
        return Err(format!("Line {start}: a quoted text is never closed."));
    }
    if !field.is_empty() || !fields.is_empty() || was_quoted {
        finish_record(&mut fields, &mut field, &mut was_quoted, start, &mut out);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEAD: &str = "book,reference,question,A,B,C,D,correct\n";

    #[test]
    fn parses_basic_rows_and_letter_answers() {
        let bank = parse(&format!("{HEAD}1 Samuel,1:1,Who?,Ana,Beto,Cal,Dan,B\n2 Reyes,2:2,Open?,,,,,Free text\n")).unwrap();
        assert_eq!(bank.rows.len(), 2);
        assert_eq!(bank.rows[0].answer, "B · Beto");
        assert!(bank.rows[0].has_options());
        assert_eq!(bank.rows[1].answer, "Free text");
        assert!(!bank.rows[1].has_options());
        assert_eq!(bank.books(), ["1 Samuel", "2 Reyes"]);
        assert_eq!(bank.per_book(), [("1 Samuel".to_string(), 1), ("2 Reyes".to_string(), 1)]);
    }
    #[test]
    fn handles_quotes_doubled_quotes_commas_and_embedded_newlines() {
        let csv = format!("{HEAD}1 Samuel,\"1:1, 2\",\"Said \"\"hi\"\",\nthen left\",A,B,,,\"Open, answer\"\r\n");
        let bank = parse(&csv).unwrap();
        assert_eq!(bank.rows[0].reference, "1:1, 2");
        assert_eq!(bank.rows[0].text, "Said \"hi\",\nthen left");
        assert_eq!(bank.rows[0].answer, "Open, answer");
    }
    #[test]
    fn accepts_crlf_bom_blank_lines_and_case_insensitive_header() {
        let csv = "\u{feff}Book,REFERENCE,Question,a,b,c,d,Correct\r\n\r\nX,r,q1,1,2,3,4,d\r\nX,r,q2,,,,,ok";
        let bank = parse(csv).unwrap();
        assert_eq!(bank.rows.len(), 2);
        assert_eq!(bank.rows[0].answer, "D · 4");
        assert_eq!(decode(b"\xEF\xBB\xBFabc").unwrap(), "abc");
        assert!(decode(&[0xff, 0xfe, 0x41]).unwrap_err().contains("UTF-8"));
    }
    #[test]
    fn semicolon_files_get_a_clear_error() {
        let err = parse("book;reference;question;a;b;c;d;correct\nx;y;z;1;2;3;4;A\n").unwrap_err();
        assert!(err.contains("Save as CSV UTF-8 with commas"));
    }
    #[test]
    fn reports_missing_header_columns() {
        let err = parse("book,question,correct\nx,y,z\n").unwrap_err();
        assert!(err.contains("Missing columns") && err.contains("reference") && err.contains(", a,"));
    }
    #[test]
    fn row_errors_name_lines_and_are_capped() {
        let err = parse(&format!("{HEAD}x,r,,,,,,ok\nx,r,q,1,2,3,4,\nx,r,q\nx,r,q,1,,3,4,B\n")).unwrap_err();
        assert!(err.contains("Line 2: question is empty"));
        assert!(err.contains("Line 3: correct answer is empty"));
        assert!(err.contains("Line 4: expected 8 columns but found 3"));
        assert!(err.contains("Line 5: correct is B but option B is empty"));
        let many: String = (0..30).map(|_| "x,r,,,,,,ok\n").collect();
        let err = parse(&format!("{HEAD}{many}")).unwrap_err();
        assert_eq!(err.matches("Line ").count(), MAX_ERRORS);
        assert!(err.contains("and 20 more"));
    }
    #[test]
    fn unclosed_quote_and_empty_files_error() {
        assert!(parse(&format!("{HEAD}x,r,\"open,1,2,3,4,A\n")).unwrap_err().contains("never closed"));
        assert!(parse("").is_err());
        assert!(parse(HEAD).unwrap_err().contains("no questions"));
    }
}
