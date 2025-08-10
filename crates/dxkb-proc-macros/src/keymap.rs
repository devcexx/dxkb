use std::collections::HashMap;

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote};

use crate::{KeyRef, dxkb_keyboard_symbol};

#[allow(non_snake_case)]
fn build_usb_keyboard_usage_ref<A: ToTokens>(key: A) -> TokenStream {
    let KeyboardUsage = dxkb_keyboard_symbol("KeyboardUsage");

    quote! {
        #KeyboardUsage::#key
    }
}

// TODO proc-macro2 indent and tokenstream types doesn't suport send or sync apparently (?)
thread_local! {
    static KNOWN_STANDARD_KEY_ALIASES: HashMap<KeyRef, Ident> = {
        let mut standard_key_aliases = HashMap::new();
        let span = Span::call_site();
        standard_key_aliases.insert(KeyRef::ident("A"), Ident::new("KeyboardAa", span));
        standard_key_aliases.insert(KeyRef::ident("B"), Ident::new("KeyboardBb", span));
        standard_key_aliases.insert(KeyRef::ident("C"), Ident::new("KeyboardCc", span));
        standard_key_aliases.insert(KeyRef::ident("D"), Ident::new("KeyboardDd", span));
        standard_key_aliases.insert(KeyRef::ident("E"), Ident::new("KeyboardEe", span));
        standard_key_aliases.insert(KeyRef::ident("F"), Ident::new("KeyboardFf", span));
        standard_key_aliases.insert(KeyRef::ident("G"), Ident::new("KeyboardGg", span));
        standard_key_aliases.insert(KeyRef::ident("H"), Ident::new("KeyboardHh", span));
        standard_key_aliases.insert(KeyRef::ident("I"), Ident::new("KeyboardIi", span));
        standard_key_aliases.insert(KeyRef::ident("J"), Ident::new("KeyboardJj", span));
        standard_key_aliases.insert(KeyRef::ident("K"), Ident::new("KeyboardKk", span));
        standard_key_aliases.insert(KeyRef::ident("L"), Ident::new("KeyboardLl", span));
        standard_key_aliases.insert(KeyRef::ident("M"), Ident::new("KeyboardMm", span));
        standard_key_aliases.insert(KeyRef::ident("N"), Ident::new("KeyboardNn", span));
        standard_key_aliases.insert(KeyRef::ident("O"), Ident::new("KeyboardOo", span));
        standard_key_aliases.insert(KeyRef::ident("P"), Ident::new("KeyboardPp", span));
        standard_key_aliases.insert(KeyRef::ident("Q"), Ident::new("KeyboardQq", span));
        standard_key_aliases.insert(KeyRef::ident("R"), Ident::new("KeyboardRr", span));
        standard_key_aliases.insert(KeyRef::ident("S"), Ident::new("KeyboardSs", span));
        standard_key_aliases.insert(KeyRef::ident("T"), Ident::new("KeyboardTt", span));
        standard_key_aliases.insert(KeyRef::ident("U"), Ident::new("KeyboardUu", span));
        standard_key_aliases.insert(KeyRef::ident("V"), Ident::new("KeyboardVv", span));
        standard_key_aliases.insert(KeyRef::ident("W"), Ident::new("KeyboardWw", span));
        standard_key_aliases.insert(KeyRef::ident("X"), Ident::new("KeyboardXx", span));
        standard_key_aliases.insert(KeyRef::ident("Y"), Ident::new("KeyboardYy", span));
        standard_key_aliases.insert(KeyRef::ident("Z"), Ident::new("KeyboardZz", span));

        standard_key_aliases.insert(KeyRef::litnum(1), Ident::new("Keyboard1Exclamation", span));
        standard_key_aliases.insert(KeyRef::litnum(2), Ident::new("Keyboard2At", span));
        standard_key_aliases.insert(KeyRef::litnum(3), Ident::new("Keyboard3Hash", span));
        standard_key_aliases.insert(KeyRef::litnum(4), Ident::new("Keyboard4Dollar", span));
        standard_key_aliases.insert(KeyRef::litnum(5), Ident::new("Keyboard5Percent", span));
        standard_key_aliases.insert(KeyRef::litnum(6), Ident::new("Keyboard6Caret", span));
        standard_key_aliases.insert(KeyRef::litnum(7), Ident::new("Keyboard7Ampersand", span));
        standard_key_aliases.insert(KeyRef::litnum(8), Ident::new("Keyboard8Asterisk", span));
        standard_key_aliases.insert(KeyRef::litnum(9), Ident::new("Keyboard9OpenParens", span));
        standard_key_aliases.insert(KeyRef::litnum(0), Ident::new("Keyboard0CloseParens", span));

        standard_key_aliases
    };
}

pub fn translate_standard_key_ref_into_hid_key(key: &KeyRef) -> TokenStream {
    let key_tokens =
        KNOWN_STANDARD_KEY_ALIASES.with(|map| map.get(key).map(build_usb_keyboard_usage_ref));

    key_tokens.unwrap_or_else(|| {
        build_usb_keyboard_usage_ref(Ident::new(
            &format!("Keyboard{}", &key.to_string()),
            Span::call_site(),
        ))
    })
}
