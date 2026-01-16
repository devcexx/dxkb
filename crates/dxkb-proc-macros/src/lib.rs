use proc_macro2::{Delimiter, Group, Span, TokenStream, TokenTree};
use quote::{ToTokens, TokenStreamExt, quote};
use std::rc::Rc;
use syn::{
    Ident, LitInt, LitStr, Path, Token, braced, bracketed,
    parse::{Parse, ParseStream, Parser},
    punctuated::Punctuated,
    spanned::Spanned,
};

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

#[derive(Debug, Clone)]
enum KeyAction {
    Passthrough(Span),
    Key(TokenStream),
}

/// A [`KeyAction`] that has been computed, taking into account any parent
/// layer, which removes the "Passthrough" key action. This enum was useful when
/// it used to be more than one action available for each key. Now, any key is
/// considered the same, and the alias translation is delegated in a proc macro,
/// but leaving it in case I need to add more variants in the future.
#[derive(Debug, Clone)]
enum ConcreteKeyAction {
    Key(TokenStream),
}
impl Parse for KeyAction {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if let Ok(t) = input.parse::<Token![*]>() {
            return Ok(KeyAction::Passthrough(t.span));
        }

        // Read every token until the next comma appears.
        let key_tokens = input.step(|cursor| {
            let mut rest = *cursor;
            let mut read = TokenStream::new();
            while let Some((tt, next)) = rest.token_tree() {
                match &tt {
                    TokenTree::Punct(punct) if punct.as_char() == ',' => {
                        return Ok((read, rest));
                    }
                    tt => {
                        read.append(tt.clone());
                        rest = next
                    }
                }
            }
            Ok((read, rest))
        })?;

        Ok(KeyAction::Key(key_tokens))
    }
}

#[derive(Debug)]
enum AttrValue {
    Str(LitStr),
    Int(LitInt),
    BracketGroup(Group),
    Path(Path),
}

impl AttrValue {
    fn span(&self) -> Span {
        match self {
            AttrValue::Str(v) => v.span(),
            AttrValue::Int(v) => v.span(),
            AttrValue::BracketGroup(v) => v.span(),
            AttrValue::Path(v) => v.span(),
        }
    }

    fn str_value(&self) -> Option<&LitStr> {
        match self {
            AttrValue::Str(lit_str) => Some(lit_str),
            _ => None,
        }
    }

    fn int_value(&self) -> Option<&LitInt> {
        match self {
            AttrValue::Int(lit_int) => Some(lit_int),
            _ => None,
        }
    }

    fn bracket_group_value(&self) -> Option<&Group> {
        match self {
            AttrValue::BracketGroup(grp) => Some(grp),
            _ => None,
        }
    }

    fn path_value(&self) -> Option<&Path> {
        match self {
            AttrValue::Path(path) => Some(path),
            _ => None,
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
        if let Ok(p) = input.parse::<Path>() {
            return Ok(AttrValue::Path(p));
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

    fn require_value_str(&self) -> syn::Result<&LitStr> {
        self.value.str_value().ok_or_else(|| {
            syn::Error::new(
                self.value.span(),
                format!("Expected string value for attribute {}", self.key_name()),
            )
        })
    }

    fn require_bracket_group(&self) -> syn::Result<&Group> {
        self.value.bracket_group_value().ok_or_else(|| {
            syn::Error::new(
                self.value.span(),
                format!("Expected array value for attribute {}", self.key_name()),
            )
        })
    }

    fn require_value_int(&self) -> syn::Result<&LitInt> {
        self.value.int_value().ok_or_else(|| {
            syn::Error::new(
                self.value.span(),
                format!("Expected int value for attribute {}", self.key_name()),
            )
        })
    }

    fn require_value_path(&self) -> syn::Result<&Path> {
        self.value.path_value().ok_or_else(|| {
            syn::Error::new(
                self.value.span(),
                format!(
                    "Expected a member path value for attribute {}",
                    self.key_name()
                ),
            )
        })
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

struct AttributeSet {
    span: Span,
    attrs: Punctuated<Attr, Token![,]>,
}

impl AttributeSet {
    fn new(span: Span, attrs: Punctuated<Attr, Token![,]>) -> Self {
        Self { span, attrs }
    }

    fn span(&self) -> Span {
        self.span
    }

    fn find_attr(&self, attr_name: &str) -> Option<&Attr> {
        self.attrs
            .iter()
            .find(|attr| attr.key.to_string().as_str() == attr_name)
    }

    fn require_attr(&self, attr_name: &str) -> syn::Result<&Attr> {
        self.find_attr(attr_name).ok_or_else(|| {
            syn::Error::new(
                self.span(),
                format!("Required attribute in set not found: {}", attr_name),
            )
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

        let content;
        let braces = braced!(content in input);
        let attrs = AttributeSet::new(
            braces.span.join(),
            content.parse_terminated(Attr::parse, Token![,])?,
        );

        let name_attr = attrs
            .require_attr(ATTR_NAME)
            .and_then(|a| a.require_value_str())?;
        let parent_attr = if let Some(attr) = attrs.find_attr(ATTR_PARENT) {
            Some(attr.require_value_str()?)
        } else {
            None
        };

        let rows_attr = attrs
            .require_attr(ATTR_ROWS)
            .and_then(|a| a.require_bracket_group())?;

        Ok(LayerDef {
            span: braces.span.span(),
            rows_span: rows_attr.span(),
            name: name_attr.clone(),
            parent: parent_attr.cloned(),
            rows: Self::parse_rows_from_group(rows_attr.clone())?,
        })
    }
}

#[derive(Debug)]
struct LayersDef<K> {
    resolver: Option<Path>,
    layers: Vec<LayerDef<K>>,
}

#[derive(Debug)]
struct ResolvedLayersDef<K> {
    resolver: Option<Path>,
    num_cols: usize,
    layers: Vec<Rc<ResolvedLayerDef<K>>>,
}

impl LayersDef<KeyAction> {
    pub fn parse_from_stream(outer_span: Span, input: TokenStream) -> syn::Result<Self> {
        fn do_parse_layers(input: ParseStream) -> syn::Result<Vec<LayerDef<KeyAction>>> {
            Ok(input
                .parse_terminated(LayerDef::parse, Token![,])?
                .into_iter()
                .collect::<Vec<_>>())
        }

        fn do_parse_attrs(input: ParseStream) -> syn::Result<Punctuated<Attr, Token![,]>> {
            input.parse_terminated(Attr::parse, Token![,])
        }

        const ATTR_RESOLVER: &str = "alias_resolver";
        const ATTR_LAYERS: &str = "layers";

        let attrs = AttributeSet::new(outer_span, Parser::parse2(do_parse_attrs, input)?);

        let alias_resolver_attr = if let Some(attr) = attrs.find_attr(ATTR_RESOLVER) {
            Some(attr.require_value_path()?)
        } else {
            None
        };

        let layers_attr = attrs
            .require_attr(ATTR_LAYERS)
            .and_then(|a| a.require_bracket_group())?;

        Ok(LayersDef {
            resolver: alias_resolver_attr.cloned(),
            layers: Parser::parse2(do_parse_layers, layers_attr.stream())?,
        })
    }

    /// Takes the raw layers definition provided by the user via the proc macro,
    /// and makes the required checks to convert the current struct into a
    /// ResolvedLayersDef. These checks include:
    ///  - The layer name is unique across the set of layers.
    ///  - Each layer has the same dimensions.
    ///  - The parents of each layer exist.
    ///  - There's no cyclic dependencies between layers.
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
                resolver: self.resolver.clone(),
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
            resolver: self.resolver.clone(),
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
                    KeyAction::Key(tt) => ConcreteKeyAction::Key(tt.clone()),
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
                    KeyAction::Key(tt) => Ok(ConcreteKeyAction::Key(tt.clone())),
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

    /// From the current provided tree (or trees), of layers, flatten each
    /// layer, by computing each [`KeyAction`], into its corresponding
    /// [`ConcreteKeyAction`]. For example, if a layer contains a set of
    /// "pass-through" KeyActions in it, they will be converted to a
    /// ConcreteKeyAction, that points to the actual Key that will be present at
    /// runtime, without need to reference the parents to determine which key
    /// will be actually present in that spot.
    fn flatten(&self) -> syn::Result<ResolvedLayersDef<ConcreteKeyAction>> {
        // Will accumulate the already flattened layers to prevent needing to
        // compute the same tree of layers more than once.
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
            resolver: self.resolver.clone(),
            num_cols: self.num_cols,
            layers: r.oks,
        })
    }
}

impl ToTokens for ConcreteKeyAction {
    #[allow(non_snake_case)]
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let layout_key_ref = match self {
            ConcreteKeyAction::Key(tt) => quote! {
                dxkb_core::default_key_from_alias!(#tt)
            },
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

fn key_to_tokens(resolver: &Path, key: &ConcreteKeyAction) -> TokenStream {
    match key {
        ConcreteKeyAction::Key(tt) => quote! {
            #resolver!(#tt)
        },
    }
}

fn layer_row_into_tokens(resolver: &Path, layer: &LayerRow<ConcreteKeyAction>) -> TokenStream {
    let LayerRow = dxkb_keyboard_symbol("LayerRow");
    let actions = layer
        .actions
        .iter()
        .map(|action| key_to_tokens(resolver, action))
        .collect::<Vec<_>>();
    quote! {
        #LayerRow::new([
            #(#actions),*
        ])
    }
}

fn layer_into_tokens(resolver: &Path, layer: &ResolvedLayerDef<ConcreteKeyAction>) -> TokenStream {
    let LayoutLayer = dxkb_keyboard_symbol("LayoutLayer");
    let rows = layer
        .rows
        .iter()
        .map(|row| layer_row_into_tokens(resolver, row));
    quote! {
        #LayoutLayer::new([
            #(#rows),*
        ])
    }
}

impl ResolvedLayersDef<ConcreteKeyAction> {
    #[allow(non_snake_case)]
    fn gen_layers_code(&self) -> proc_macro2::TokenStream {
        let resolver = self.resolver.clone().unwrap_or_else(|| {
            syn::parse2::<Path>(quote! { dxkb_core::default_key_from_alias }).unwrap()
        });
        let layers = &self
            .layers
            .iter()
            .map(|layer| layer_into_tokens(&resolver, layer))
            .collect::<Vec<_>>();
        quote! {
            [
                #(#layers),*
            ]
        }
    }
}

#[proc_macro]
pub fn layers(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let stream: proc_macro2::TokenStream = item.into();
    let input = match LayersDef::parse_from_stream(stream.span(), stream) {
        Ok(r) => r,
        Err(e) => return e.to_compile_error().into(),
    };

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
