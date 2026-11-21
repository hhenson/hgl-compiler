# Source text

`source-text.md`

## LEX-1

lexer: done

- Identifier form; reserved words and the lone `_` are their own token kinds

## LEX-2

lexer: done

- String tokens; unknown escape, unclosed string and unclosed comment rejected

## LEX-3

lexer: done

- Line and block comments are tokens; `//` is a symbol

## LEX-4

parser: done

- Braces delimit blocks; indentation ignored; `;` only in native hook markers

## LEX-5

parser: done

- A line break is ignored in a list, in an incomplete statement, and before a token that cannot begin one; two or more always separate

## LEX-6

parser: done

- Two statements on one line rejected; an inject list continues after a comma

## LEX-7

lexer: done

- Minus is its own token; integer and float literals checked against i64 and f64

analyser: done

- 2^63 is an integer only directly under a minus; an enum's numbers are held as `i64`, the least included

## LEX-8

lexer: done

- `>` never joined to `>`; digits then letters are one duration or tenor candidate; exponent forms

## LEX-9

parser: done

- Any word of identifier form, reserved words included, as member name and argument label; `const` as modifier or name by position

## LEX-10

parser: done

- Contextual names take their meaning by position; `str(value)` is a conversion and `error(...)` a constructor
- `run` is a hard reserved word, as `eval` is, and names nothing

## LEX-11

lexer: done

- Date, time and offset fields; civil and zoned forms

## LEX-12

lexer: done

- Zone name source form

transpiler: planned

## LEX-13

lexer: done

- Duration units, their order, the final fraction, whole microseconds

## LEX-14

lexer: done

- `<=>` is one token

parser: done

- `<=>` continues a line as the other binary operators do

## LEX-15

lexer: done

- LF, CRLF and CR are each one line break, in the line index and in every token

## LEX-16

lexer: done

- UTF-8 check; leading U+FEFF dropped; whitespace; a character that begins no token rejected

## LEX-17

lexer: done

- The longest symbol is taken; `>=` is one token even after generic arguments

parser: done

- `>=` is not split to close generic arguments

## LEX-18

lexer: done

- A malformed numeric, duration or temporal run is one rejected token

## LEX-19

lexer: done

- Offset hours 00 to 23, minutes 00 to 59

## LEX-20

lexer: done

- Float value nearest, ties to even; infinity rejected; underflow accepted

## LEX-21

lexer: done

- Block and documentation comments are tokens; `/**/` is an empty block comment

parser: done

- A comment containing a line break counts as one line break

## LEX-22

lexer: done

- One `_` between two digits

## LEX-23

lexer: done

- Hexadecimal and binary integers

## LEX-24

lexer: done

- `\u{...}` escapes

## LEX-25

lexer: done

- Text blocks

## LEX-26

lexer: done

- `\` and the line break after it are one continuation token; any other `\` is rejected

parser: done

- A continued line break is no line break

## LEX-27

parser: done

- The opening bracket of a call, parameter list, index, generic list or cardinality directly follows what it applies to

## LEX-28

lexer: done

- Tenor units, uppercase: `Y`, `M`, `W`, `D` once each in that order, or `B` alone; whole numbers; a year twelve months and a week seven days; each count in the i64 domain (`tenor.invalid`)

parser: done

- A tenor literal is a literal expression, and may begin one
