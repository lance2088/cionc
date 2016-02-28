use std::str::Chars;
use std::rc::Rc;

use util::is_any_of::*;

use parser::token::*;
use parser::token_stream::TokenStream;
use parser::util::char_util::CharProperties;
use parser::compile_context::CompileContext;
use parser::string_table::StringTable;

// This is the lexer implementation for the parser (that sadly doesn't exist yet).
// I am fully aware of super-neat tools that can generate lexers and parser automatically,
// however, I want to implement this to learn about Rust and because I also want full control
// over things in my code base.
// Besides that, ... I like the Rust's match expressions. :)
// Keep in mind that this implementation isn't final as many things like scan_string_literal(...)
// are still missing or are not completely implemented, yet (like scan_char_literal(...)).

pub struct Lexer<'input, 'ctx> {
	context: &'ctx CompileContext,
	input: Chars<'input>,
	buffer: String,
	cur_char: char
}

impl<'input, 'ctx> Lexer<'input, 'ctx> {
	pub fn new(
		ctx: &'ctx CompileContext,
		iterator: Chars<'input>
	)
		-> Lexer<'input, 'ctx>
	{
		let mut lexer = Lexer {
			context: ctx,
			input: iterator,
			buffer: String::new(),
			cur_char: '\0' };
		lexer.consume();
		lexer
	}

	pub fn new_from_str<'content: 'input>(
		ctx: &'ctx CompileContext,
		content : &'content str
	)
		-> Lexer<'input, 'ctx>
	{
		Lexer::new(ctx, content.chars())
	}

	/// Stores the current character into the buffer
	/// and returns reference to self for method chaining
	fn store(&mut self) -> &mut Self {
		self.buffer.push(self.cur_char);
		self
	}

	/// Consumes the next character unwraped and
	/// returns reference to self for method chaining
	fn consume(&mut self) -> &mut Self {
		self.cur_char = self.input.next().unwrap_or('\0');
		self
	}

	/// Returns the char from input which was read last
	fn get(&self) -> char {
		self.cur_char
	}

	/// Returns the given token, used as helper method
	/// for method chaining in order to improve the code-flow
	/// May be more important in future versions for managing
	/// of source locations.
	fn make(&self, token: Token) -> Token {
		token
	}

	/// Clears all chars in the buffer for special tokens
	/// and returns reference to self for method chaining
	fn clear_buffer(&mut self) -> &mut Self {
		self.buffer.clear();
		self
	}

	/// Drains the content of this buffer by performing
	/// a trial insertion at the context's StringTable.
	/// This buffer is empty after this operation!
	fn drain_buffer(&mut self) -> Rc<String> {
		let rc = self.context.get_string_table().get_or_insert(&self.buffer);
		self.clear_buffer();
		rc
	}

	fn scan_line_comment(&mut self) -> Token {
		assert_eq!(self.get(), '/');
		self.skip_while(|c| c.is_none_of(&['\n','\0']));
		self.consume();
		self.make(Token::Comment)
	}

	fn scan_multi_line_comment(&mut self) -> Token {
		assert_eq!(self.get(), '*');
		self.consume();
		loop {
			match self.get() {
				'*' => match self.consume().get() {
					'/' => return self.consume().make(Token::Comment),
					'*' => continue,
					_   => self.consume()
				},
				'\0' => return self.make(Token::Error),
				_ => self.consume()
			};
		}
	}

	fn scan_identifier(&mut self) -> Token {
		assert!(self.get().is_alpha());
		self.store_while(|c| c.is_alpha_numeral() || c == '_');
		let drained = self.drain_buffer();
		self.make(Token::Identifier(drained))

		// omg this doesn't work because the borrow-checker
		// can not handle situations like this properly at the moment
		// self.make(Token::Identifier(self.drain_buffer()))
	}

	fn scan_char_literal(&mut self) -> Token {
		assert_eq!(self.get(), '\'');
		match self.consume().get() {
			/* error: empty character literal */
			'\'' => self.make(Token::Error),

			/* escape characters */
			'\\' => match self.store().consume().get() {
				/* special escape characters */
				'n'  |
				't'  |
				'r'  |
				'\\' |
				'\'' => match self.store().consume().get() {
					'\'' => {
						self.consume();
						let drained = self.drain_buffer();
						self.make(Token::Literal(LiteralToken::Char(drained)))
					},
					_ => self.make(Token::Error)
				},

				/* hex-code unicode followed by two hex-digits */
				'x' => match self.store().consume().get() {
					/* error: no hex-digits provided */
					'\'' => self.make(Token::Error),

					/* valid unicode starting code-point */
					'0' ... '7' => match self.store().consume().get() {
						/* error: just one unicode code-point given */
						'\'' => self.make(Token::Error),

						/* valid unicode 2nd code-point given */
						'0' ... '9' |
						'a' ... 'f' |
						'A' ... 'F' => match self.store().consume().get() {
							/* valid closed unicode char literal */
							'\'' => {
								let drained = self.drain_buffer();
								self.make(Token::Literal(
									LiteralToken::Char(drained)))
							},
							/* error: has to close after two hex-digits */
							_ => self.make(Token::Error)
						},

						/* error: invalid 2nd code-point */
						_ => self.make(Token::Error)
					},

					/* invalid starting points for unicode */
					'8' ... '9' |
					'a' ... 'f' |
					'A' ... 'F' => self.make(Token::Error),

					/* anything else invalid */
					_ => self.make(Token::Error)
				},

				/* uni-code up to 6 hex-digits (TODO) */
				'u' => match self.store().consume().get() {
					_ => self.make(Token::Error)
				},

				/* no valid escape character read */
				_ => self.make(Token::Error)
			},

			/* normal ascii charater literal */
			_ => match self.store().consume().get() {
				'\'' => {
					let drained = self.drain_buffer();
					self.consume().make(Token::Literal(
						LiteralToken::Char(drained)))
				},
				_ => self.make(Token::Error) // more than one code-point in character literal
			}
		}
	}

	fn scan_string_literal(&mut self) -> Token {
		self.make(Token::Error)
	}

	fn scan_integral_literal_suffix(&mut self) -> Token {
		self.store_while(|c| c.is_alpha_numeral());
		let drained = self.drain_buffer();
		self.make(Token::Literal(
			LiteralToken::Integer(drained)))
	}

	fn scan_binary_literal(&mut self) -> Token {
		assert_eq!(self.get(), 'b');
		self.store().consume();
		self.store_while(|c| c.is_binary_numeral() || c == '_');
		// self.scan_integral_literal_suffix()
		let drained = self.drain_buffer();
		self.make(Token::Literal(
			LiteralToken::Integer(drained)))
	}

	fn scan_octal_literal(&mut self) -> Token {
		assert_eq!(self.get(), 'o');
		self.store().consume();
		self.store_while(|c| c.is_octal_numeral() || c == '_');
		// self.scan_integral_literal_suffix()
		let drained = self.drain_buffer();
		self.make(Token::Literal(
			LiteralToken::Integer(drained)))
	}

	fn scan_hexdec_literal(&mut self) -> Token {
		assert_eq!(self.get(), 'x');
		self.store().consume();
		self.store_while(|c| c.is_hexdec_numeral() || c == '_');
		// self.scan_integral_literal_suffix()
		let drained = self.drain_buffer();
		self.make(Token::Literal(
			LiteralToken::Integer(drained)))
	}

	fn scan_decimal_literal(&mut self) -> Token {
		assert!(self.get().is_decimal_numeral() || self.get() == '_');
		self.store_while(|c| c.is_decimal_numeral() || c == '_');
		match self.get() {
			'.' => self.scan_float_literal(),
			_ => {
				let drained = self.drain_buffer();
				self.make(Token::Literal(
					LiteralToken::Integer(drained)))
			}
		}
	}

	fn scan_float_literal(&mut self) -> Token {
		assert_eq!(self.get(), '.');
		Token::EndOfFile
	}

	fn scan_number_literal(&mut self) -> Token {
		assert!(self.get().is_decimal_numeral());
		match self.get() {
			'0' => match self.store().consume().get() {
				'b' => self.scan_binary_literal(),
				'o' => self.scan_octal_literal(),
				'x' => self.scan_hexdec_literal(),
				'.' => self.scan_float_literal(),

				/* dec number literals cannot start with '0' */
				'0' ... '9' | '_' => self.scan_decimal_literal(),

				/* literal number suffix, e.g. 0i32 */
				c if c.is_alpha() => self.scan_integral_literal_suffix(),

				/* */
				_ => self.make(Token::Error)
			},

			/* decimal number literal */
			'1' ... '9' => self.scan_decimal_literal(),

			_ => unreachable!()
		}
	}

	/// Stores all characters from input as long as they fullfill the given predicate
	/// and returns reference to self for method chaining
	fn store_while<P>(&mut self, predicate: P) -> &mut Self
		where P: Fn(char) -> bool
	{
		while predicate(self.get()) {
			self.store().consume();
		}
		self
	}

	/// Skips all characters from input as long as they fullfill the given predicate
	/// and returns reference to self for method chaining
	fn skip_while<P>(&mut self, predicate: P) -> &mut Self
		where P: Fn(char) -> bool
	{
		while predicate(self.get()) {
			self.consume();
		}
		self
	}
}

impl<'input, 'ctx> TokenStream for Lexer<'input, 'ctx> {
	fn next_token(&mut self) -> Token {
		self.clear_buffer();
		match self.get() {
			/* Skip whitespace */
			c if c.is_whitespace() => {
				self.skip_while(|c| c.is_whitespace());
				self.make(Token::Whitespace)
			},

			/* Opening delimiters */
			'(' => self.consume().make(Token::OpenDelim(DelimitToken::Paren)),
			'[' => self.consume().make(Token::OpenDelim(DelimitToken::Bracket)),
			'{' => self.consume().make(Token::OpenDelim(DelimitToken::Brace)),

			/* Opening delimiters */
			')' => self.consume().make(Token::CloseDelim(DelimitToken::Paren)),
			']' => self.consume().make(Token::CloseDelim(DelimitToken::Bracket)),
			'}' => self.consume().make(Token::CloseDelim(DelimitToken::Brace)),

			/* Special tokens which aren't the beginning
			   of any other token */
			'?' => self.consume().make(Token::Question),
			';' => self.consume().make(Token::SemiColon),
			',' => self.consume().make(Token::Comma),
			'_' => self.consume().make(Token::Underscore),

			/* Dot, DotDot and DotDotDot tokens */
			'.' => match self.consume().get() {
				'.' => match self.consume().get() {
					'.' => self.consume().make(Token::DotDotDot),
					_   => self.make(Token::DotDot)
				},
				_ => self.make(Token::Dot)
			},

			/* Tokens starting with '+' */
			'+' => match self.consume().get() {
				'=' => self.consume().make(Token::BinOpEq(BinOpToken::Plus)),
				_   => self.make(Token::BinOp(BinOpToken::Plus))
			},

			/* Tokens starting with '-' */
			'-' => match self.consume().get() {
				'=' => self.consume().make(Token::BinOpEq(BinOpToken::Minus)),
				'>' => self.consume().make(Token::Arrow),
				_   => self.make(Token::BinOp(BinOpToken::Minus))
			},

			/* Tokens starting with '*' */
			'*' => match self.consume().get() {
				'=' => self.consume().make(Token::BinOpEq(BinOpToken::Star)),
				_   => self.make(Token::BinOp(BinOpToken::Star))
			},

			/* Tokens starting with '/' */
			'/' => match self.consume().get() {
				'=' => self.consume().make(Token::BinOpEq(BinOpToken::Slash)),
				'/' => self.scan_line_comment(),
				'*' => self.scan_multi_line_comment(),
				_ => self.make(Token::BinOp(BinOpToken::Slash))
			},

			/* Tokens starting with '%' */
			'%' => match self.consume().get() {
				'=' => self.consume().make(Token::BinOpEq(BinOpToken::Percent)),
				_   => self.make(Token::BinOp(BinOpToken::Percent))
			},

			/* Tokens starting with '^' */
			'^' => match self.consume().get() {
				'=' => self.consume().make(Token::BinOpEq(BinOpToken::Caret)),
				_   => self.make(Token::BinOp(BinOpToken::Caret))
			},

			/* Tokens starting with '!' */
			'!' => match self.consume().get() {
				'=' => self.consume().make(Token::RelOp(RelOpToken::NotEq)),
				_   => self.make(Token::Exclamation)
			},

			/* Tokens starting with '=' */
			'=' => match self.consume().get() {
				'>' => self.consume().make(Token::FatArrow),
				'=' => self.consume().make(Token::RelOp(RelOpToken::EqEq)),
				_   => self.make(Token::Eq)
			},

			/* Tokens starting with '&' */
			'&' => match self.consume().get() {
				'&' => self.consume().make(Token::LogicalOp(LogicalOpToken::AndAnd)),
				'=' => self.consume().make(Token::BinOpEq(BinOpToken::And)),
				_   => self.make(Token::BinOp(BinOpToken::And))
			},

			/* Tokens starting with '|' */
			'|' => match self.consume().get() {
				'|' => self.consume().make(Token::LogicalOp(LogicalOpToken::OrOr)),
				'=' => self.consume().make(Token::BinOpEq(BinOpToken::Or)),
				_   => self.make(Token::BinOp(BinOpToken::Or))
			},

			/* Tokens starting with '<' */
			'<' => match self.consume().get() {
				'<' => match self.consume().get() {
					'=' => self.consume().make(Token::BinOpEq(BinOpToken::Shl)),
					_   => self.make(Token::BinOp(BinOpToken::Shl))
				},
				'=' => self.consume().make(Token::RelOp(RelOpToken::LessEq)),
				_   => self.make(Token::RelOp(RelOpToken::LessThan))
			},

			/* Tokens starting with '>' */
			'>' => match self.consume().get() {
				'>' => match self.consume().get() {
					'=' => self.consume().make(Token::BinOpEq(BinOpToken::Shr)),
					_   => self.make(Token::BinOp(BinOpToken::Shr))
				},
				'=' => self.consume().make(Token::RelOp(RelOpToken::GreaterEq)),
				_   => self.make(Token::RelOp(RelOpToken::GreaterThan))
			},

			/* Char and string literals */
			'\'' => self.scan_char_literal(),
			'\"' => self.scan_string_literal(),

			/* Integer- and float literals and identifiers */
			c if c.is_decimal_numeral() => self.scan_number_literal(),

			/* Identifiers and keywords */
			c if c.is_alpha() => self.scan_identifier(),

			/* When end of iterator has been reached */
			_ => self.make(Token::EndOfFile)
		}
	}
}

impl<'input, 'ctx> Iterator for Lexer<'input, 'ctx> {
	type Item = Token;

	fn next(&mut self) -> Option<Self::Item> {
		let token = self.next_token();
		match token {
			Token::EndOfFile => None,
			_                => Some(token)
		}
	}
}

#[cfg(test)]
mod tests {
	use std::rc::Rc;

	use super::*;
	use super::super::token::*;
	use super::super::compile_context::CompileContext;

	#[test]
	fn simple_tokens() {
		let solution = vec![
			Token::OpenDelim(DelimitToken::Paren),
			Token::CloseDelim(DelimitToken::Paren),
			
			Token::OpenDelim(DelimitToken::Bracket),
			Token::CloseDelim(DelimitToken::Bracket),
			
			Token::OpenDelim(DelimitToken::Brace),
			Token::CloseDelim(DelimitToken::Brace),

			Token::Question,
			Token::SemiColon,
			Token::Comma,

			Token::EndOfFile
		];
		let ctx   = CompileContext::default();
		let lexer = Lexer::new_from_str(&ctx, "()[]{}?;,");
		for zipped in solution.into_iter().zip(lexer) {
			assert_eq!(zipped.0, zipped.1);
		}
	}

	#[test]
	fn simple_comments() {
		let solution = vec![
			Token::Comment,
			Token::Whitespace,
			Token::Comment,
			Token::Whitespace,
			Token::Comment
		];
		let ctx   = CompileContext::default();
		let lexer = Lexer::new_from_str(&ctx,
			"//Hello, World!
			/*Ignored new line!\nBlub!\nSee?*/
			/****multiple stars don't hurt****/");
		for zipped in solution.into_iter().zip(lexer) {
			assert_eq!(zipped.0, zipped.1);
		}
	}

	#[test]
	fn simple_dots() {
		let solution = vec![
			Token::DotDot,
			Token::Whitespace,
			Token::DotDotDot,
			Token::Comment,
			Token::Dot
		];
		let ctx   = CompileContext::default();
		let lexer = Lexer::new_from_str(&ctx, "..\t.../*Useless comment*/.");
		for zipped in solution.into_iter().zip(lexer) {
			assert_eq!(zipped.0, zipped.1);
		}
	}

	#[test]
	fn simple_char_literal() {
		let solution = vec![
			Token::char_literal_from_str("c"),
			Token::Whitespace,
			Token::char_literal_from_str("\n"),
			Token::Whitespace,
			Token::char_literal_from_str("\t"),
			Token::Whitespace,
			Token::char_literal_from_str("\x7F")
		];
		let ctx   = CompileContext::default();
		let lexer = Lexer::new_from_str(&ctx, r"'c' '\n' '\t' '\x7F'");
		for zipped in solution.into_iter().zip(lexer) {
			assert_eq!(zipped.0, zipped.1);
		}
	}

	#[test]
	fn simple_whitespace() {
		let solution = vec![Token::Whitespace];
		let ctx   = CompileContext::default();
		let lexer = Lexer::new_from_str(&ctx, " \t\r\n");
		for zipped in solution.into_iter().zip(lexer) {
			assert_eq!(zipped.0, zipped.1);
		}
	}

	#[test]
	fn simple_integral_literals() {
		let solution = vec![
			Token::integer_literal_from_str("0b1011_0010_0000_0001"),
			Token::Whitespace,
			Token::integer_literal_from_str("0o731_312_645_003"),
			Token::Whitespace,
			Token::integer_literal_from_str("0xFF_AE_03_95"),
			Token::Whitespace,
			Token::integer_literal_from_str("987654321"),
		];
		let ctx   = CompileContext::default();
		let lexer = Lexer::new_from_str(&ctx,
			"0b1011_0010_0000_0001
			 0o731_312_645_003
			 0xFF_AE_03_95
			 987654321");
		for zipped in solution.into_iter().zip(lexer) {
			assert_eq!(zipped.0, zipped.1);
		}
	}

	#[test]
	fn simple_float_literals() {
		let solution = vec![
			Token::float_literal_from_str("0.0"),
			Token::Whitespace,
			Token::float_literal_from_str("42.0"),
			Token::Whitespace,
			Token::float_literal_from_str("0.24"),
			Token::Whitespace,
			Token::float_literal_from_str("13.37"),
			Token::Whitespace,
			Token::float_literal_from_str("0.00001"),
			Token::Whitespace,
		];
		let ctx = CompileContext::default();
		let lexer = Lexer::new_from_str(&ctx,
			"0.0
			 42.0
			 0.24
			 13.37
			 0.00001");
		for zipped in solution.into_iter().zip(lexer) {
			assert_eq!(zipped.0, zipped.1);
		}
	}

	#[test]
	fn simple_identifiers() {
		let solution = vec![
			Token::identifier_from_str("true"),
			Token::Whitespace,
			Token::identifier_from_str("false"),
			Token::Whitespace,
			Token::identifier_from_str("alphanumeric"),
			Token::Whitespace,
			Token::identifier_from_str("with_underscore"),
			Token::Whitespace,
			Token::identifier_from_str("underscores_at_the_end__"),
			Token::Whitespace,
			Token::identifier_from_str("with_n0m3r5")
		];
		let ctx = CompileContext::default();
		let lexer = Lexer::new_from_str(&ctx,
			"true false
			 alphanumeric
			 with_underscore
			 underscores_at_the_end__
			 with_n0m3r5");
		for zipped in solution.into_iter().zip(lexer) {
			assert_eq!(zipped.0, zipped.1);
		}
	}
}
