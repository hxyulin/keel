//! Common metadata emission for source assembly and the in-process API.
use crate::Format;
pub(crate) use keel_asm_ir::{Cfi, Directive, Expression, SectionKind};
use std::fmt::Write;

pub(crate) trait Sink: Write {
    fn directive(&mut self, d: Directive);
    fn label(&mut self, name: String);
    fn section(&mut self, segment: &str, name: &str, kind: SectionKind) {
        self.directive(Directive::Section {
            segment: segment.into(),
            name: name.into(),
            kind,
        });
    }
    fn debug_section(&mut self, format: Format, name: &str) {
        if format == Format::Elf {
            self.directive(Directive::ElfSection {
                name: format!(".debug_{name}"),
                flags: 0,
                nobits: false,
            });
        } else {
            self.section("__DWARF", &format!("__debug_{name}"), SectionKind::Debug);
        }
    }
    fn integer(&mut self, width: u8, expression: Expression) {
        self.directive(Directive::Integer {
            width,
            expression,
            got: false,
        });
    }
    fn number(&mut self, width: u8, value: impl Value) {
        self.integer(width, Expression::constant(value.value()));
    }
    fn bytes(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.directive(Directive::Bytes(bytes.to_vec()));
        }
    }
    fn cstring(&mut self, value: &str) {
        self.directive(Directive::CString(value.as_bytes().to_vec()));
    }
    fn leb(&mut self, signed: bool, expression: Expression) {
        self.directive(Directive::Leb { signed, expression });
    }
    fn align(&mut self, power: u32) {
        self.directive(Directive::Align(power.try_into().expect("valid alignment")));
    }
}
pub(crate) trait Value {
    fn value(self) -> i128;
}
macro_rules! values{($($t:ty),*)=>{$(impl Value for $t{fn value(self)->i128{self as i128}}impl Value for &$t{fn value(self)->i128{*self as i128}})*};}
values!(u8, u16, u32, u64, usize, i8, i16, i32, i64, i128, isize);
impl Sink for String {
    fn directive(&mut self, d: Directive) {
        writeln!(self, "\t{d}").unwrap();
    }
    fn label(&mut self, name: String) {
        writeln!(self, "{name}:").unwrap();
    }
}
