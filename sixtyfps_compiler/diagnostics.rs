/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2020 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2020 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Debug, Clone, Default)]
pub struct Span {
    pub offset: usize,
    #[cfg(feature = "proc_macro_span")]
    pub span: Option<proc_macro::Span>,
}

impl PartialEq for Span {
    fn eq(&self, other: &Span) -> bool {
        self.offset == other.offset
    }
}

impl Span {
    pub fn new(offset: usize) -> Self {
        Self { offset, ..Default::default() }
    }
}

#[cfg(feature = "proc_macro_span")]
impl From<proc_macro::Span> for Span {
    fn from(span: proc_macro::Span) -> Self {
        Self { span: Some(span), ..Default::default() }
    }
}

/// Returns a span.  This is implemented for tokens and nodes
pub trait Spanned {
    fn span(&self) -> crate::diagnostics::Span;
}

pub type SourceFile = Rc<PathBuf>;

pub trait SpannedWithSourceFile: Spanned {
    fn source_file(&self) -> Option<&SourceFile>;
}

#[derive(Debug)]
pub enum Level {
    Error,
    Warning,
}

impl Default for Level {
    fn default() -> Self {
        Self::Error
    }
}

#[cfg(feature = "display-diagnostics")]
impl From<Level> for codemap_diagnostic::Level {
    fn from(l: Level) -> Self {
        match l {
            Level::Error => codemap_diagnostic::Level::Error,
            Level::Warning => codemap_diagnostic::Level::Warning,
        }
    }
}

#[derive(thiserror::Error, Default, Debug)]
#[error("{message}")]
pub struct CompilerDiagnostic {
    pub message: String,
    pub span: Span,
    pub level: Level,
}

#[derive(thiserror::Error, Debug)]
pub enum Diagnostic {
    #[error(transparent)]
    FileLoadError(#[from] std::io::Error),
    #[error(transparent)]
    CompilerDiagnostic(#[from] CompilerDiagnostic),
}

#[derive(Default, Debug)]
pub struct FileDiagnostics {
    pub inner: Vec<Diagnostic>,
    pub current_path: SourceFile,
    pub source: Option<String>,
}

impl IntoIterator for FileDiagnostics {
    type Item = Diagnostic;
    type IntoIter = <Vec<Diagnostic> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl FileDiagnostics {
    pub fn push_diagnostic_with_span(&mut self, message: String, span: Span, level: Level) {
        self.inner.push(CompilerDiagnostic { message, span, level }.into());
    }
    pub fn push_error_with_span(&mut self, message: String, span: Span) {
        self.push_diagnostic_with_span(message, span, Level::Error)
    }
    pub fn push_error(&mut self, message: String, source: &dyn Spanned) {
        self.push_error_with_span(message, source.span());
    }
    pub fn push_compiler_error(&mut self, error: CompilerDiagnostic) {
        self.inner.push(error.into());
    }

    pub fn has_error(&self) -> bool {
        self.inner.iter().any(|diag| match diag {
            Diagnostic::FileLoadError(_) => true,
            Diagnostic::CompilerDiagnostic(diag) => matches!(diag.level, Level::Error),
        })
    }

    #[cfg(feature = "display-diagnostics")]
    fn emit_diagnostics<'a, Output>(
        self,
        output: &'a mut Output,
        emitter_factory: impl for<'b> FnOnce(
            &'b mut Output,
            Option<&'b codemap::CodeMap>,
        ) -> codemap_diagnostic::Emitter<'b>,
    ) {
        let mut codemap = codemap::CodeMap::new();
        let internal_errors = self.source.is_none();
        let file = codemap.add_file(
            self.current_path.to_string_lossy().to_string(),
            self.source.unwrap_or_default(),
        );
        let file_span = file.span;

        let diags: Vec<_> = self
            .inner
            .into_iter()
            .map(|diagnostic| match diagnostic {
                Diagnostic::CompilerDiagnostic(CompilerDiagnostic { message, span, level }) => {
                    let spans = if !internal_errors {
                        let s = codemap_diagnostic::SpanLabel {
                            span: file_span.subspan(span.offset as u64, span.offset as u64),
                            style: codemap_diagnostic::SpanStyle::Primary,
                            label: None,
                        };
                        vec![s]
                    } else {
                        vec![]
                    };
                    codemap_diagnostic::Diagnostic {
                        level: level.into(),
                        message,
                        code: None,
                        spans,
                    }
                }
                Diagnostic::FileLoadError(err) => codemap_diagnostic::Diagnostic {
                    level: codemap_diagnostic::Level::Error,
                    message: err.to_string(),
                    code: None,
                    spans: vec![],
                },
            })
            .collect();

        let mut emitter = emitter_factory(output, Some(&codemap));
        emitter.emit(&diags);
    }

    #[cfg(feature = "display-diagnostics")]
    /// Print the diagnostics on the console
    pub fn print(self) {
        self.emit_diagnostics(&mut (), |_, codemap| {
            codemap_diagnostic::Emitter::stderr(codemap_diagnostic::ColorConfig::Always, codemap)
        });
    }

    #[cfg(feature = "display-diagnostics")]
    /// Print into a string
    pub fn diagnostics_as_string(self) -> String {
        let mut output = Vec::new();
        self.emit_diagnostics(&mut output, |output, codemap| {
            codemap_diagnostic::Emitter::vec(output, codemap)
        });

        String::from_utf8(output).expect(&format!(
            "Internal error: There were errors during compilation but they did not result in valid utf-8 diagnostics!"

        ))
    }

    #[cfg(feature = "display-diagnostics")]
    pub fn check_and_exit_on_error(self) -> Self {
        if self.has_error() {
            self.print();
            std::process::exit(-1);
        }
        self
    }

    #[cfg(feature = "proc_macro_span")]
    /// Will convert the diagnostics that only have offsets to the actual span
    pub fn map_offsets_to_span(&mut self, span_map: &[crate::parser::Token]) {
        for d in &mut self.inner {
            if let Diagnostic::CompilerDiagnostic(d) = d {
                if d.span.span.is_none() {
                    //let pos =
                    //span_map.binary_search_by_key(d.span.offset, |x| x.0).unwrap_or_else(|x| x);
                    //d.span.span = span_map.get(pos).as_ref().map(|x| x.1);
                    let mut offset = 0;
                    d.span.span = span_map.iter().find_map(|t| {
                        if d.span.offset <= offset {
                            t.span
                        } else {
                            offset += t.text.len();
                            None
                        }
                    });
                }
            }
        }
    }

    pub fn to_string_vec(&self) -> Vec<String> {
        self.inner.iter().map(|d| d.to_string()).collect()
    }

    pub fn new_from_error(path: std::path::PathBuf, err: std::io::Error) -> Self {
        Self { inner: vec![err.into()], current_path: Rc::new(path), source: None }
    }
}

#[cfg(feature = "proc_macro_span")]
use quote::quote;

#[cfg(feature = "proc_macro_span")]
impl quote::ToTokens for FileDiagnostics {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let diags: Vec<_> = self
            .inner
            .iter()
            .filter_map(|diag| match diag {
                Diagnostic::CompilerDiagnostic(CompilerDiagnostic {
                    level,
                    message,
                    span,
                }) => {
                    match level {
                        Level::Error => {
                            if let Some(span) = span.span {
                                Some(quote::quote_spanned!(span.into() => compile_error!{ #message }))
                            } else {
                                Some(quote!(compile_error! { #message }))
                            }
                        }
                        Level::Warning => {
                            let warning_symbol = quote::format_ident!("WARNING_{}", message.replace("-", "_"));
                            let warning = quote!(
                                #[warn(dead_code)]
                                #[allow(non_upper_case_globals)]
                                const #warning_symbol : () = ();
                            );
                            if let Some(span) = span.span {
                                Some(quote::quote_spanned!(span.into() => #warning))
                            } else {
                                Some(warning)
                            }                            
                        }
                    }
                },                
                _ => None,
            })
            .collect();
        quote!(#(#diags)*).to_tokens(tokens);
    }
}

#[derive(Default)]
pub struct BuildDiagnostics {
    per_input_file_diagnostics: HashMap<SourceFile, FileDiagnostics>,
    internal_errors: Option<FileDiagnostics>,
}

impl BuildDiagnostics {
    pub fn add(&mut self, diagnostics: FileDiagnostics) {
        match self.per_input_file_diagnostics.get_mut(&diagnostics.current_path) {
            Some(existing_diags) => existing_diags.inner.extend(diagnostics.inner),
            None => {
                self.per_input_file_diagnostics
                    .insert(diagnostics.current_path.clone(), diagnostics);
            }
        }
    }

    pub fn push_diagnostic(
        &mut self,
        message: String,
        source: &dyn SpannedWithSourceFile,
        level: Level,
    ) {
        match source.source_file() {
            Some(source_file) => self
                .per_input_file_diagnostics
                .entry(source_file.clone())
                .or_insert_with(|| FileDiagnostics {
                    current_path: source_file.clone(),
                    ..Default::default()
                })
                .push_diagnostic_with_span(message, source.span(), level),
            None => self.push_internal_error(
                CompilerDiagnostic { message, span: source.span(), level }.into(),
            ),
        }
    }

    pub fn push_error(&mut self, message: String, source: &dyn SpannedWithSourceFile) {
        self.push_diagnostic(message, source, Level::Error)
    }

    pub fn push_internal_error(&mut self, err: Diagnostic) {
        self.internal_errors
            .get_or_insert_with(|| FileDiagnostics {
                current_path: Rc::new("[internal error]".into()),
                ..Default::default()
            })
            .inner
            .push(err)
    }

    fn iter(&self) -> impl Iterator<Item = &FileDiagnostics> {
        self.per_input_file_diagnostics.values().chain(self.internal_errors.iter())
    }

    pub fn into_iter(self) -> impl Iterator<Item = FileDiagnostics> {
        self.per_input_file_diagnostics
            .into_iter()
            .map(|(_, diag)| diag)
            .chain(self.internal_errors.into_iter())
    }

    #[cfg(feature = "proc_macro_span")]
    fn iter_mut(&mut self) -> impl Iterator<Item = &mut FileDiagnostics> {
        self.per_input_file_diagnostics.values_mut().chain(self.internal_errors.iter_mut())
    }

    pub fn has_error(&self) -> bool {
        self.iter().any(|diag| diag.has_error())
    }

    pub fn to_string_vec(&self) -> Vec<String> {
        self.iter()
            .flat_map(|diag| {
                diag.to_string_vec()
                    .iter()
                    .map(|err| format!("{}: {}", diag.current_path.to_string_lossy(), err))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[cfg(feature = "display-diagnostics")]
    pub fn print(self) {
        self.into_iter().for_each(|diag| diag.print());
    }

    #[cfg(feature = "display-diagnostics")]
    pub fn check_and_exit_on_error(self) -> Self {
        if self.has_error() {
            self.print();
            std::process::exit(-1);
        }
        self
    }

    #[cfg(feature = "display-diagnostics")]
    pub fn print_warnings_and_exit_on_error(self) {
        let has_error = self.has_error();
        self.print();
        if has_error {
            std::process::exit(-1);
        }
    }

    #[cfg(feature = "proc_macro_span")]
    pub fn map_offsets_to_span(&mut self, span_map: &[crate::parser::Token]) {
        self.iter_mut().for_each(|diag| diag.map_offsets_to_span(span_map))
    }
}

#[cfg(feature = "proc_macro_span")]
impl quote::ToTokens for BuildDiagnostics {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.iter().for_each(|diag| diag.to_tokens(tokens))
    }
}

impl Extend<FileDiagnostics> for BuildDiagnostics {
    fn extend<T: IntoIterator<Item = FileDiagnostics>>(&mut self, iter: T) {
        for diag in iter {
            self.add(diag)
        }
    }
}
