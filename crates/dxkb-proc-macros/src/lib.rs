use proc_macro2::{Delimiter, Group, Span, TokenStream};
use quote::{ToTokens, TokenStreamExt, quote};
use std::rc::Rc;
use syn::{
    Ident, LitInt, LitStr, Token, braced, bracketed,
    parse::{Parse, ParseStream, Parser},
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
};

mod keymap;

struct ResultAcc<T, E> {
    oks: Vec<T>,
    errors: Vec<E>,
}

impl<A, E> FromIterator<Result<A, E>> for ResultAcc<A, E> {
    fn from_iter<T: IntoIterator<Item = Result<A, E>>>(iter: T) -> Self {
        let iter = iter.into_iter();

        let mut oks = Vec::with_capacity(iter.size_hint().0);
        let mut errors = Vec::new();

        for e in iter {
            match e {
                Ok(v) => oks.push(v),
                Err(e) => errors.push(e),
            }
        }

        ResultAcc { oks, errors }
    }
}

fn combine_syn_errors(errors: &[syn::Error]) -> Option<syn::Error> {
    let Some(mut head) = errors.first().cloned() else {
        return None;
    };
    for e in &errors[1..] {
        head.combine(e.clone());
    }

    Some(head)
}

fn dxkb_keyboard_symbol(name: &str) -> TokenStream {
    let ident = Ident::new(name, Span::call_site());
    quote! {
        ::dxkb_core::keyboard::#ident
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub(crate) enum KeyRef {
    Ident(String),
    LitInt(u32),
    LitChr(char),
}

impl From<LitStr> for KeyRef {
    fn from(value: LitStr) -> Self {
        KeyRef::Ident(value.value())
    }
}

impl From<Ident> for KeyRef {
    fn from(value: Ident) -> Self {
        KeyRef::Ident(value.to_string())
    }
}

impl KeyRef {
    pub fn ident(str: &str) -> KeyRef {
        Self::Ident(str.to_string())
    }

    pub fn litnum(n: u32) -> KeyRef {
        Self::LitInt(n)
    }

    pub fn litchr(ch: char) -> KeyRef {
        Self::LitChr(ch)
    }
}

impl ToString for KeyRef {
    fn to_string(&self) -> String {
        match self {
            KeyRef::Ident(ident) => ident.clone(),
            KeyRef::LitInt(int) => int.to_string(),
            KeyRef::LitChr(c) => c.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
enum KeyAction {
    Passthrough(Span),
    NoOp(Span),
    StandardKey(KeyRef),
    FunctionKey(KeyRef),
}

/// A [`KeyAction`] that has been computed, taking into account any
/// parent layer, which removes the "Passthrough" key action.
#[derive(Debug, Clone)]
enum ConcreteKeyAction {
    NoOp,
    StandardKey(KeyRef),
    FunctionKey(KeyRef),
}
impl Parse for KeyAction {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if let Ok(t) = input.parse::<Token![*]>() {
            return Ok(KeyAction::Passthrough(t.span));
        }
        if let Ok(t) = input.parse::<Token![_]>() {
            return Ok(KeyAction::NoOp(t.span));
        }

        if let Ok(r) = input.parse::<LitInt>() {
            if input.is_empty() || input.peek(Token![,]) {
                return Ok(Self::StandardKey(KeyRef::LitInt(r.base10_parse().unwrap())));
            }
        }

        let first_ident = input.parse::<Ident>()?;
        if input.is_empty() || input.peek(Token![,]) {
            // EOF, this should be a standard key
            Ok(KeyAction::StandardKey(KeyRef::Ident(
                first_ident.to_string(),
            )))
        } else {
            input.parse::<Token![:]>()?;
            let key_ref = input.parse::<Ident>()?;
            Ok(KeyAction::FunctionKey(KeyRef::Ident(key_ref.to_string())))
        }
    }
}

#[derive(Debug)]
enum AttrValue {
    Str(LitStr),
    Int(LitInt),
    BracketGroup(Group),
}

impl AttrValue {
    fn span(&self) -> Span {
        match self {
            AttrValue::Str(v) => v.span(),
            AttrValue::Int(v) => v.span(),
            AttrValue::BracketGroup(v) => v.span(),
        }
    }
}

impl Parse for AttrValue {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if let Ok(str) = input.parse::<LitStr>() {
            return Ok(AttrValue::Str(str));
        }
        if let Ok(int) = input.parse::<LitInt>() {
            return Ok(AttrValue::Int(int));
        }
        if let Ok(g) = input.parse::<Group>() {
            if g.delimiter() == Delimiter::Bracket {
                return Ok(AttrValue::BracketGroup(g));
            }
        }

        Err(syn::Error::new(
            input.span(),
            "Unrecognized attribute value type",
        ))
    }
}

#[derive(Debug)]
struct Attr {
    key: Ident,
    value: AttrValue,
}

impl Attr {
    fn key_name(&self) -> String {
        return format!("{}", self.key);
    }

    fn require_value_str(&self) -> syn::Result<LitStr> {
        match &self.value {
            AttrValue::Str(lit_str) => Ok(lit_str.clone()),
            _ => Err(syn::Error::new(
                self.value.span(),
                format!("Expected string value for attribute {}", self.key_name()),
            )),
        }
    }

    fn require_value_bracket_group(&self) -> syn::Result<Group> {
        match &self.value {
            AttrValue::BracketGroup(group) => Ok(group.clone()),
            _ => Err(syn::Error::new(
                self.value.span(),
                format!(
                    "Expected curly brackets value for attribute {}",
                    self.key_name()
                ),
            )),
        }
    }
}

impl Parse for Attr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let key = input.parse::<Ident>()?;
        input.parse::<Token![:]>()?;
        let value = input.parse::<AttrValue>()?;
        Ok(Attr { key, value })
    }
}

struct AttrSetDef {
    attrs: Punctuated<Attr, Token![,]>,
}

impl Parse for AttrSetDef {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(AttrSetDef {
            attrs: input.parse_terminated(Attr::parse, Token![,])?,
        })
    }
}

#[derive(Debug, Clone)]
struct LayerRow<K> {
    span: Span,
    actions: Vec<K>,
}

impl Parse for LayerRow<KeyAction> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        let bracket = bracketed!(content in input);
        let actions = content
            .parse_terminated(KeyAction::parse, Token![,])?
            .into_iter()
            .collect::<Vec<_>>();

        Ok(LayerRow {
            span: bracket.span.span(),
            actions,
        })
    }
}

#[derive(Debug, Clone)]
struct LayerDef<K> {
    span: Span,
    rows_span: Span,
    name: LitStr,
    parent: Option<LitStr>,
    rows: Vec<LayerRow<K>>,
}

#[derive(Debug, Clone)]
struct ResolvedLayerDef<K> {
    name: String,
    parent: Option<Rc<ResolvedLayerDef<K>>>,
    rows: Vec<LayerRow<K>>,
}

impl LayerDef<KeyAction> {
    // TODO Implement Parse instead
    fn parse_rows_from_group(group: Group) -> syn::Result<Vec<LayerRow<KeyAction>>> {
        fn do_parse_rows(input: ParseStream) -> syn::Result<Vec<LayerRow<KeyAction>>> {
            Ok(input
                .parse_terminated(LayerRow::parse, Token![,])?
                .into_iter()
                .collect::<Vec<_>>())
        }

        let input = group.stream();

        Ok(Parser::parse(do_parse_rows, input.into())?)
    }
}

impl Parse for LayerDef<KeyAction> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        const ATTR_NAME: &str = "name";
        const ATTR_PARENT: &str = "parent";
        const ATTR_ROWS: &str = "rows";

        fn ensure_unset<A: Spanned>(
            current_attr: &Attr,
            attr_value_holder: &Option<A>,
        ) -> syn::Result<()> {
            if let Some(lit) = attr_value_holder {
                let mut e = syn::Error::new(
                    current_attr.key.span(),
                    "Attribute value already set previously.",
                );
                e.combine(syn::Error::new(lit.span(), "Previously set here"));
                return Err(e);
            }

            Ok(())
        }

        fn require_attr<A>(span: Span, attr: &str, holder: Option<A>) -> syn::Result<A> {
            holder.ok_or_else(|| {
                syn::Error::new(
                    span,
                    format!("Required attribute not found in layer definition: {}", attr),
                )
            })
        }

        let content;
        let braces = braced!(content in input);
        let attrs = content.parse::<AttrSetDef>()?.attrs;

        let mut name_attr: Option<LitStr> = None;
        let mut parent_attr: Option<LitStr> = None;
        let mut rows_attr: Option<Group> = None;

        for attr in attrs.into_iter() {
            match attr.key_name().as_str() {
                ATTR_NAME => {
                    ensure_unset(&attr, &name_attr)?;
                    name_attr.replace(attr.require_value_str()?);
                }
                ATTR_PARENT => {
                    ensure_unset(&attr, &parent_attr)?;
                    parent_attr.replace(attr.require_value_str()?);
                }
                ATTR_ROWS => {
                    ensure_unset(&attr, &rows_attr)?;
                    rows_attr.replace(attr.require_value_bracket_group()?);
                }
                value => {
                    return Err(syn::Error::new(
                        attr.value.span(),
                        format!("Unknown attribute in layer definition: {}", value),
                    ));
                }
            }
        }

        let rows = require_attr(content.span(), &ATTR_ROWS, rows_attr)?;

        Ok(LayerDef {
            span: braces.span.span(),
            rows_span: rows.span(),
            name: require_attr(content.span(), &ATTR_NAME, name_attr)?,
            parent: parent_attr,
            rows: Self::parse_rows_from_group(rows)?,
        })
    }
}

#[derive(Debug)]
struct LayersDef<K> {
    layers: Vec<LayerDef<K>>,
}

#[derive(Debug)]
struct ResolvedLayersDef<K> {
    num_cols: usize,
    layers: Vec<Rc<ResolvedLayerDef<K>>>,
}

impl Parse for LayersDef<KeyAction> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(LayersDef {
            layers: input
                .parse_terminated(LayerDef::parse, Token![,])?
                .into_iter()
                .collect::<Vec<_>>(),
        })
    }
}

impl LayersDef<KeyAction> {
    pub fn resolve_references(&self) -> syn::Result<ResolvedLayersDef<KeyAction>> {
        fn find_resolved(
            name: &str,
            layer_acc: &mut Vec<Rc<ResolvedLayerDef<KeyAction>>>,
        ) -> Option<Rc<ResolvedLayerDef<KeyAction>>> {
            layer_acc
                .iter()
                .find(|resolved_layer| &resolved_layer.name == name)
                .cloned()
        }

        fn resolve_layer(
            this: &LayersDef<KeyAction>,
            source_layer: &LayerDef<KeyAction>,
            current_path: &mut Vec<String>,
            resolved_layers: &mut Vec<Rc<ResolvedLayerDef<KeyAction>>>,
        ) -> syn::Result<Rc<ResolvedLayerDef<KeyAction>>> {
            let resolved = find_resolved(&source_layer.name.value(), resolved_layers);
            match resolved {
                Some(resolved) => Ok(resolved),
                None => {
                    let resolved_parent;
                    if let Some(parent_name) = &source_layer.parent {
                        let Some(parent) = this
                            .layers
                            .iter()
                            .find(|layer| &layer.name.value() == &parent_name.value())
                        else {
                            return Err(syn::Error::new(
                                parent_name.span(),
                                format!(
                                    "Couldn't find a layer with name '{}'",
                                    &parent_name.value()
                                ),
                            ));
                        };
                        let cycle_found = current_path.contains(&parent.name.value());
                        current_path.push(parent.name.value());
                        if cycle_found {
                            return Err(syn::Error::new(
                                parent_name.span(),
                                format!(
                                    "Cyclic dependency found between layers: {}",
                                    current_path.join(" -> ")
                                ),
                            ));
                        }
                        resolved_parent =
                            Some(resolve_layer(this, parent, current_path, resolved_layers)?);
                        current_path.pop();
                    } else {
                        resolved_parent = None;
                    }

                    let resolved = Rc::new(ResolvedLayerDef {
                        name: source_layer.name.value(),
                        parent: resolved_parent,
                        rows: source_layer.rows.clone(),
                    });

                    resolved_layers.push(Rc::clone(&resolved));
                    Ok(resolved)
                }
            }
        }

        fn track_visited_layer(defined_layers: &mut Vec<String>, next: &LitStr) -> syn::Result<()> {
            let name = next.value();
            if defined_layers.contains(&name) {
                Err(syn::Error::new(
                    next.span(),
                    format!("Layer already defined: {}", &name),
                ))
            } else {
                defined_layers.push(name);
                Ok(())
            }
        }

        fn ensure_rows_same_length(layer: &LayerDef<KeyAction>) -> syn::Result<usize> {
            let Some(expected_cols) = layer.rows.first().map(|r| r.actions.len()) else {
                return Ok(0);
            };

            for row in layer.rows.iter() {
                if row.actions.len() != expected_cols {
                    return Err(syn::Error::new(
                        row.span,
                        format!(
                            "Expected every row to have the same dimension. Expected {} elements, but {} got.",
                            expected_cols,
                            row.actions.len()
                        ),
                    ));
                }
            }

            Ok(expected_cols)
        }

        let mut resolved_layers = Vec::new();
        let mut already_defined_layers = Vec::new();

        let Some(first_layer) = self.layers.first() else {
            return Ok(ResolvedLayersDef {
                num_cols: 0,
                layers: vec![],
            });
        };

        let expected_col_count = ensure_rows_same_length(first_layer)?;
        let expected_row_count = first_layer.rows.len();

        let r = self.layers.iter().map(|layer| {
            let col_count = ensure_rows_same_length(layer)?;
            if expected_col_count != col_count || expected_row_count != layer.rows.len() {
                return Err(syn::Error::new(layer.rows_span, format!("Expected every layer to have the same dimensions as the firstly defined layer. Expected a layer of {}x{}, but found {}x{}.", expected_row_count, expected_col_count, layer.rows.len(), col_count)));
            }


            track_visited_layer(&mut already_defined_layers, &layer.name)?;
            Ok(resolve_layer(self, layer, &mut Vec::new(), &mut resolved_layers)?)
        }).collect::<ResultAcc<_, _>>();

        if let Some(error) = combine_syn_errors(&r.errors) {
            return Err(error);
        }

        Ok(ResolvedLayersDef {
            num_cols: expected_col_count,
            layers: r.oks,
        })
    }
}

impl ResolvedLayersDef<KeyAction> {
    fn flatten_with_parent(
        layer: &ResolvedLayerDef<KeyAction>,
        parent: &Rc<ResolvedLayerDef<ConcreteKeyAction>>,
    ) -> ResolvedLayerDef<ConcreteKeyAction> {
        fn flatten_row(
            row: &LayerRow<KeyAction>,
            row_idx: usize,
            parent: &Rc<ResolvedLayerDef<ConcreteKeyAction>>,
        ) -> LayerRow<ConcreteKeyAction> {
            let actions = row
                .actions
                .iter()
                .enumerate()
                .map(|(action_idx, action)| match action {
                    KeyAction::Passthrough(_) => parent.rows[row_idx].actions[action_idx].clone(),
                    KeyAction::NoOp(_) => ConcreteKeyAction::NoOp,
                    KeyAction::StandardKey(key_ref) => {
                        ConcreteKeyAction::StandardKey(key_ref.clone())
                    }
                    KeyAction::FunctionKey(key_ref) => {
                        ConcreteKeyAction::StandardKey(key_ref.clone())
                    }
                })
                .collect::<Vec<_>>();

            LayerRow {
                span: row.span,
                actions,
            }
        }

        let rows = layer
            .rows
            .iter()
            .enumerate()
            .map(|(row_index, row)| flatten_row(&row, row_index, parent))
            .collect::<Vec<_>>();

        ResolvedLayerDef {
            name: layer.name.clone(),
            parent: Some(Rc::clone(parent)),
            rows,
        }
    }

    fn flatten_no_parent(
        layer: &ResolvedLayerDef<KeyAction>,
    ) -> syn::Result<ResolvedLayerDef<ConcreteKeyAction>> {
        fn flatten_row(row: &LayerRow<KeyAction>) -> syn::Result<LayerRow<ConcreteKeyAction>> {
            let r = row
                .actions
                .iter()
                .map(|action| match action {
                    KeyAction::Passthrough(span) => Err(syn::Error::new(
                        *span,
                        format!("Cannot use the passthrough action on a layer with no parent"),
                    )),
                    KeyAction::NoOp(_) => Ok(ConcreteKeyAction::NoOp),
                    KeyAction::StandardKey(key_ref) => {
                        Ok(ConcreteKeyAction::StandardKey(key_ref.clone()))
                    }
                    KeyAction::FunctionKey(key_ref) => {
                        Ok(ConcreteKeyAction::StandardKey(key_ref.clone()))
                    }
                })
                .collect::<ResultAcc<_, _>>();
            if let Some(err) = combine_syn_errors(&r.errors) {
                return Err(err);
            }
            Ok(LayerRow {
                span: row.span,
                actions: r.oks,
            })
        }

        let r = layer
            .rows
            .iter()
            .map(|row| flatten_row(row))
            .collect::<ResultAcc<_, _>>();

        if let Some(err) = combine_syn_errors(&r.errors) {
            return Err(err);
        }

        Ok(ResolvedLayerDef {
            name: layer.name.clone(),
            parent: None,
            rows: r.oks,
        })
    }

    fn flatten_layer(
        &self,
        layer: &Rc<ResolvedLayerDef<KeyAction>>,
        flattened_layers: &mut Vec<Rc<ResolvedLayerDef<ConcreteKeyAction>>>,
    ) -> syn::Result<Rc<ResolvedLayerDef<ConcreteKeyAction>>> {
        let result_layer;
        if let Some(parent) = &layer.parent {
            let flattened_parent;
            if let Some(existing_parent) = flattened_layers.iter().find(|l| &l.name == &parent.name)
            {
                flattened_parent = Rc::clone(existing_parent);
            } else {
                flattened_parent = self.flatten_layer(parent, flattened_layers)?;
            }

            result_layer = Rc::new(Self::flatten_with_parent(layer, &flattened_parent));
        } else {
            result_layer = Rc::new(Self::flatten_no_parent(layer)?);
        }
        flattened_layers.push(Rc::clone(&result_layer));
        Ok(result_layer)
    }

    fn flatten(&self) -> syn::Result<ResolvedLayersDef<ConcreteKeyAction>> {
        let mut layers_acc = Vec::new();

        let r = self
            .layers
            .iter()
            .map(|layer| self.flatten_layer(layer, &mut layers_acc))
            .collect::<ResultAcc<_, _>>();

        if let Some(err) = combine_syn_errors(&r.errors) {
            return Err(err);
        }

        Ok(ResolvedLayersDef {
            num_cols: self.num_cols,
            layers: r.oks,
        })
    }
}

impl ToTokens for ConcreteKeyAction {
    #[allow(non_snake_case)]
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let LayoutKey = quote! {
            ::dxkb_core::def_key::DefaultKey
        };
        let layout_key_ref = match self {
            ConcreteKeyAction::NoOp => quote! {
                #LayoutKey::NoOp
            },
            ConcreteKeyAction::StandardKey(key_ref) => {
                let key_tokens = keymap::translate_standard_key_ref_into_hid_key(key_ref);

                quote! {
                    #LayoutKey::Standard(#key_tokens)
                }
            }
            ConcreteKeyAction::FunctionKey(key_ref) => {
                todo!("Still need to figure out if I like the current function keys approach?")
            }
        };

        tokens.append_all(layout_key_ref);
    }
}

impl ToTokens for LayerRow<ConcreteKeyAction> {
    #[allow(non_snake_case)]
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let LayerRow = dxkb_keyboard_symbol("LayerRow");
        let actions = &self.actions;
        tokens.append_all(quote! {
            #LayerRow::new([
                #(#actions),*
            ])
        })
    }
}

impl ToTokens for ResolvedLayerDef<ConcreteKeyAction> {
    #[allow(non_snake_case)]
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let LayoutLayer = dxkb_keyboard_symbol("LayoutLayer");
        let rows = &self.rows;
        tokens.append_all(quote! {
            #LayoutLayer::new([
                #(#rows),*
            ])
        });
    }
}

impl ResolvedLayersDef<ConcreteKeyAction> {
    #[allow(non_snake_case)]
    fn gen_layers_code(&self) -> proc_macro2::TokenStream {
        let layers = &self.layers;
        quote! {
            [
                #(#layers),*
            ]
        }
    }
}

#[proc_macro]
pub fn layers(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item as LayersDef<KeyAction>);

    let layers;
    match input.resolve_references() {
        Ok(r) => {
            layers = r;
        }
        Err(e) => return e.to_compile_error().into(),
    }

    match layers.flatten() {
        Ok(r) => r.gen_layers_code().into(),
        Err(e) => return e.to_compile_error().into(),
    }
}
