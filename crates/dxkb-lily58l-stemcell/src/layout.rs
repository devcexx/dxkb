 use crate::{config::TLayout, custom_key_from_alias};

#[rustfmt::skip]
pub const LAYOUT: TLayout = TLayout::new(
    dxkb_proc_macros::layers!(
        alias_resolver: custom_key_from_alias,
        layers: [
            {   // 0
                name: "base",
                rows: [
                    [  Esc,    1,    2,    3,    4,    5,  /* | */    6,    7,    8,    9,    0,  '`'],
                    [  Tab,    Q,    W,    E,    R,    T,  /* | */    Y,    U,    I,    O,    P,  '-'],
                    [ LCtl,    A,    S,    D,    F,    G,  /* | */    H,    J,    K,    L,  ';',  "'"],
                    [ LSft,    Z,    X,    C,    V,    B,  /* | */    N,    M,  ',',  '.',  '/', '\\'],
                    [    X, LAlt, LGui,u:LEx,  Spc,  '[',  /* | */  ']',Enter,u:LFn, RAlt, Bksp,    X],
                ]
            },
            {   // 1
                name: "extend",
                parent: "base",
                rows: [
                    [   F1,    F2,   F3,   F4,   F5,   F6,  /* | */   F7,   F8,   F9,  F10,  F11,  F12],
                    [    *,     *,    *,    *,    *,    *,  /* | */    *,    *,    *,    *,    *,    *],
                    [    *,     *,    *,    *,    *,    *,  /* | */    *,    *,    *,    *,    *,    *],
                    [    *,     *,    *,    *,    *,    *,  /* | */    *,    *,    *,    *,    *,    *],
                    [    *,     *,    *,    *,    *,    *,  /* | */    *,    *,    *,    *,    *,    *],
                ]
            },
            {   // 2
                name: "function",
                parent: "base",
                rows: [
                    [    *,     *,    *,    *,    *,    *,  /* | */    *,c:Ply,c:Prv,c:Nxt,c:VDn,c:VUp],
                    [    *,     *,    *,    *,    *,    *,  /* | */ Home,PrScr,   Up,Insrt, PgUp,  '='],
                    [    *,     *,    *,    *,    *,    *,  /* | */  End, Left, Down,Right, PgDn,u:Pls],
                    [    *,     *,    *,    *,    *,    *,  /* | */    *,    *,    *,    *,    *,c:Pwr],
                    [c:Slp,     *,    *,    *,    *,    *,  /* | */    *, Caps,    *,    *,  Del,    *],
                ]
            },
        ]
    )
);
