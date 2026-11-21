//! Resolve C warning groups after parsing the specific overrides.

use keel_c::{LanguageOptions, UnusedWarnings};

#[derive(Default)]
pub(super) struct Groups {
    all: bool,
    extra: bool,
    unused: Option<bool>,
    returns: Option<bool>,
    implicit_int: Option<bool>,
    enum_compare: Option<bool>,
    enum_compare_conditional: Option<bool>,
    enum_conversion: Option<bool>,
    cast_function_type: Option<bool>,
    sign_compare: Option<bool>,
    empty_body: Option<bool>,
    pub(super) fallthrough: Option<bool>,
    unused_function: Option<bool>,
    unused_variable: Option<bool>,
    unused_const_variable: Option<bool>,
    unused_set_variable: Option<bool>,
    unused_parameter: Option<bool>,
    unused_set_parameter: Option<bool>,
    unused_label: Option<bool>,
    unused_local_typedefs: Option<bool>,
    unused_value: Option<bool>,
}

impl Groups {
    pub(super) fn parse(&mut self, arg: &str, language: &mut LanguageOptions) -> bool {
        let (name, enable) = if let Some(name) = arg.strip_prefix("-Wno-") {
            (name, false)
        } else if let Some(name) = arg.strip_prefix("-W") {
            (name, true)
        } else {
            return false;
        };
        let explicit = Some(enable);
        match name {
            "all" => self.all = enable,
            "extra" => self.extra = enable,
            "unused" => self.unused = explicit,
            "error=return-type" if enable => {
                language.error_return_type = true;
                self.returns = explicit;
            }
            "return-type" => {
                self.returns = explicit;
                if !enable {
                    language.error_return_type = false;
                }
            }
            "error=implicit-int" if enable => {
                language.error_implicit_int = true;
                self.implicit_int = explicit;
            }
            "implicit-int" => {
                self.implicit_int = explicit;
                if !enable {
                    language.error_implicit_int = false;
                }
            }
            "enum-compare" => self.enum_compare = explicit,
            "enum-compare-conditional" => self.enum_compare_conditional = explicit,
            "enum-conversion" => self.enum_conversion = explicit,
            "cast-function-type" => self.cast_function_type = explicit,
            "sign-compare" => self.sign_compare = explicit,
            "empty-body" => self.empty_body = explicit,
            "unused-function" => self.unused_function = explicit,
            "unused-variable" => self.unused_variable = explicit,
            "unused-const-variable" => self.unused_const_variable = explicit,
            "unused-but-set-variable" => self.unused_set_variable = explicit,
            "unused-parameter" => self.unused_parameter = explicit,
            "unused-but-set-parameter" => self.unused_set_parameter = explicit,
            "unused-label" => self.unused_label = explicit,
            "unused-local-typedefs" => self.unused_local_typedefs = explicit,
            "unused-value" => self.unused_value = explicit,
            _ => return false,
        }
        true
    }

    pub(super) fn apply(self, language: &mut LanguageOptions) {
        language.warn_return_type = self.returns.unwrap_or(self.all);
        language.warn_implicit_int = self.implicit_int.unwrap_or(self.all);
        language.warn_enum_compare = self.enum_compare.unwrap_or(self.all);
        language.warn_enum_compare_conditional = self
            .enum_compare_conditional
            .unwrap_or(language.warn_enum_compare);
        language.warn_enum_conversion = self.enum_conversion.unwrap_or(self.extra);
        language.warn_cast_function_type = self.cast_function_type.unwrap_or(self.extra);
        language.warn_sign_compare = self.sign_compare.unwrap_or(self.extra);
        language.warn_empty_body = self.empty_body.unwrap_or(self.extra);
        language.warn_implicit_fallthrough = self.fallthrough.unwrap_or(self.extra);
        let unused = self.unused.unwrap_or(self.all);
        let variables = self.unused_variable.unwrap_or(unused);
        language.unused = UnusedWarnings {
            functions: self.unused_function.unwrap_or(unused),
            variables,
            const_variables: self.unused_const_variable.unwrap_or(variables),
            set_variables: self.unused_set_variable.unwrap_or(unused),
            parameters: self.unused_parameter.unwrap_or(unused && self.extra),
            set_parameters: self.unused_set_parameter.unwrap_or(unused && self.extra),
            labels: self.unused_label.unwrap_or(unused),
            local_typedefs: self.unused_local_typedefs.unwrap_or(unused),
            values: self.unused_value.unwrap_or(unused),
        };
    }
}
