#[allow(unused)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyboardPageCode {
    /// Error emitted by keyboards when the number of maximum simultaneous keys pressed overflows.
    ErrorRollOver = 0x01,

    /// Keyboard POST error?
    ErrorPOSTFail = 0x02,

    /// Other keyboard error
    ErrorUndefined = 0x03,

    /// Key a and A
    A = 0x04,

    /// Key b and B
    B = 0x05,

    /// Key c and C
    C = 0x06,

    /// Key d and D
    D = 0x07,

    /// Key e and E
    E = 0x08,

    /// Key f and F
    F = 0x09,

    /// Key g and G
    G = 0x0A,

    /// Key h and H
    H = 0x0B,

    /// Key i and I
    I = 0x0C,

    /// Key j and J
    J = 0x0D,

    /// Key k and K
    K = 0x0E,

    /// Key l and L
    L = 0x0F,

    /// Key m and M
    M = 0x10,

    /// Key n and N
    N = 0x11,

    /// Key o and O
    O = 0x12,

    /// Key p and P
    P = 0x13,

    /// Key q and Q
    Q = 0x14,

    /// Key r and R
    R = 0x15,

    /// Key s and S
    S = 0x16,

    /// Key t and T
    T = 0x17,

    /// Key u and U
    U = 0x18,

    /// Key v and V
    V = 0x19,

    /// Key w and W
    W = 0x1A,

    /// Key x and X
    X = 0x1B,

    /// Key y and Y
    Y = 0x1C,

    /// Key z and Z
    Z = 0x1D,

    /// Key 1 and !
    One = 0x1E,

    /// Key 2 and @
    Two = 0x1F,

    /// Key 3 and #
    Three = 0x20,

    /// Key 4 and $
    Four = 0x21,

    /// Key 5 and %
    Five = 0x22,

    /// Key 6 and ^
    Six = 0x23,

    /// Key 7 and &
    Seven = 0x24,

    /// Key 8 and *
    Eight = 0x25,

    /// Key 9 and (
    Nine = 0x26,

    /// Key 0 and )
    Zero = 0x27,

    /// Return key
    Return = 0x28,

    /// Escape key
    Escape = 0x29,

    /// Backspace
    Backspace = 0x2A,

    /// Tab key
    Tab = 0x2B,

    /// Space bar
    SpaceBar = 0x2C,

    /// Key - and _
    Hyphen = 0x2D,

    /// Key = and +
    Equals = 0x2E,

    /// Key { and [
    BracketOpen = 0x2F,

    /// Key } and ]
    BracketClose = 0x30,

    /// Key \ and |
    Backslash = 0x31,

    /// Especial key with a different meaning depending on the locale, with usually the following mapping:
    ///   - For US: \|
    ///   - For Belgium: µ `£
    ///   - For French Canadian: <}>
    ///   - For Danish: ’*
    ///   - For Dutch: <>
    ///   - For German: # ’
    ///   - For Italian: ù §
    ///   - For Latin America: } `]
    ///   - For Norwegian: , *
    ///   - For Spain: }Ç
    ///   - For Swedish: ,*
    ///   - For Swiss: $ £
    ///   - For UK: # ~
    NonUSHash = 0x32,

    /// Key ; and :
    Semicolon = 0x33,

    /// Key ' and "
    Quote = 0x34,

    /// Key ` and ~
    Grave = 0x35,

    /// Key , and <
    Comma = 0x36,

    /// Key . and >
    Period = 0x37,

    /// Key / and ?
    Slash = 0x38,

    /// CapsLock key
    CapsLock = 0x39,

    /// Key F1
    F1 = 0x3A,

    /// Key F2
    F2 = 0x3B,

    /// Key F3
    F3 = 0x3C,

    /// Key F4
    F4 = 0x3D,

    /// Key F5
    F5 = 0x3E,

    /// Key F6
    F6 = 0x3F,

    /// Key F7
    F7 = 0x40,

    /// Key F8
    F8 = 0x41,

    /// Key F9
    F9 = 0x42,

    /// Key F10
    F10 = 0x43,

    /// Key F11
    F11 = 0x44,

    /// Key F12
    F12 = 0x45,

    /// Print screen key
    PrintScreen = 0x46,

    /// Scroll lock key
    ScrollLock = 0x47,

    /// Pause key
    Pause = 0x48,

    /// Insert Key
    Insert = 0x49,

    /// Home key
    Home = 0x4A,

    /// Page Up key
    PageUp = 0x4B,

    /// Delete forward (or just delete) key
    Delete = 0x4C,

    /// End key
    End = 0x4D,

    /// Page down Key
    PageDown = 0x4E,

    /// Right arrow
    Right = 0x4F,

    /// Left arrow
    Left = 0x50,

    /// Down arrow
    Down = 0x51,

    /// Up arrow
    Up = 0x52,

    /// Num lock and clear key
    NumLock = 0x53,

    /// Keypad / key
    KeypadSlash = 0x54,

    /// Keypad * key
    KeypadMultiply = 0x55,

    /// Keypad - key
    KeypadMinus = 0x56,

    /// Keypad + key
    KeypadPlus = 0x57,

    /// Keypad enter
    KeypadEnter = 0x58,

    /// Keypad number 1 and End key
    Keypad1 = 0x59,

    /// Keypad number 1 and down arrow key
    Keypad2 = 0x5A,

    /// Keypad number 3 and page down key
    Keypad3 = 0x5B,

    /// Keypad number 4 and left arrow key
    Keypad4 = 0x5C,

    /// Keypad number 5
    Keypad5 = 0x5D,

    /// Keypad number 6 and right arrow key
    Keypad6 = 0x5E,

    /// Keypad number 7 and home key
    Keypad7 = 0x5F,

    /// Keypad number 8 and up arrow
    Keypad8 = 0x60,

    /// Keypad 9 and page up arrow
    Keypad9 = 0x61,

    /// Keypad 0 and insert key
    Keypad0 = 0x62,

    /// Keypad period and delete key
    KeypadPeriod = 0x63,

    /// Especial key with a different meaning depending on the locale, with usually the following mapping:
    ///   - Belgium: <\>
    ///   - French Canadian: <°>
    ///   - Danish: <\>
    ///   - Dutch: ]|[
    ///   - French: <>
    ///   - German: <|>
    ///   - Italian: <>
    ///   - LatinAmerica: <>
    ///   - Norwegian: <>
    ///   - Spain: <>
    ///   - Swedish: <|>
    ///   - Swiss: <>
    ///   - UK: \|
    ///   - Brazil: \|
    NonUSBackslash = 0x64,

    /// Application key
    Application = 0x65,

    /// Power key
    Power = 0x66,

    /// Keypad equals key
    KeypadEquals = 0x67,

    /// Key F13
    F13 = 0x68,

    /// Key F14
    F14 = 0x69,

    /// Key F15
    F15 = 0x6A,

    /// Key F16
    F16 = 0x6B,

    /// Key F17
    F17 = 0x6C,

    /// Key F18
    F18 = 0x6D,

    /// Key F19
    F19 = 0x6E,

    /// Key F20
    F20 = 0x6F,

    /// Key F21
    F21 = 0x70,

    /// Key F22
    F22 = 0x71,

    /// Key F23
    F23 = 0x72,

    /// Key F24
    F24 = 0x73,

    /// Execute key
    Execute = 0x74,

    /// Help key
    Help = 0x75,

    /// Menu key
    Menu = 0x76,

    /// Select key
    Select = 0x77,

    /// Stop key
    Stop = 0x78,

    /// Again key
    Again = 0x79,

    /// Undo key
    Undo = 0x7A,

    /// Cut key
    Cut = 0x7B,

    /// Copy key
    Copy = 0x7C,

    /// Paste key
    Paste = 0x7D,

    /// Find key
    Find = 0x7E,

    /// Mute key
    Mute = 0x7F,

    /// Volume Up key
    VolumeUp = 0x80,

    /// Volume Down key
    VolumeDown = 0x81,

    /// Locking caps lock key
    LockingCapsLock = 0x82,

    /// Locking num lock key
    LockingNumLock = 0x83,

    /// Locking scroll lock key
    LockingScrollLock = 0x84,

    /// Keypad comma key
    KeypadComma = 0x85,

    /// Keypad Equal key, used in AS/400 systems.
    As400KeypadEqual = 0x86,

    /// International key 1
    Intl1 = 0x87,

    /// International key 2
    Intl2 = 0x88,

    /// International key 3
    Intl3 = 0x89,

    /// International key 4
    Intl4 = 0x8A,

    /// International key 5
    Intl5 = 0x8B,

    /// International key 6
    Intl6 = 0x8C,

    /// International key 7
    Intl7 = 0x8D,

    /// International key 8
    Intl8 = 0x8E,

    /// International key 9
    Intl9 = 0x8F,

    /// Key LANG1
    Lang1 = 0x90,

    /// Key LANG2
    Lang2 = 0x91,

    /// Key LANG3
    Lang3 = 0x92,

    /// Key LANG4
    Lang4 = 0x93,

    /// Key LANG5
    Lang5 = 0x94,

    /// Key LANG6
    Lang6 = 0x95,

    /// Key LANG7
    Lang7 = 0x96,

    /// Key LANG8
    Lang8 = 0x97,

    /// Key LANG9
    Lang9 = 0x98,

    /// Alternate erase key
    AlternateErase = 0x99,

    /// SysReq key
    SysReq = 0x9A,

    /// Cancel key
    Cancel = 0x9B,

    /// Clear key
    Clear = 0x9C,

    /// Prior key
    Prior = 0x9D,

    /// Alternate return key
    AltReturn = 0x9E,

    /// Separator key
    Separator = 0x9F,

    /// Out key
    Out = 0xA0,

    /// Oper key
    Oper = 0xA1,

    /// Clear/Again key
    ClearAgain = 0xA2,
    /// CrSel / Props key
    CrSel = 0xA3,

    /// ExSel key
    ExSel = 0xA4,

    /// Keypad 00
    Keypad00 = 0xB0,

    /// Keypad 000
    Keypad000 = 0xB1,

    /// Thousand separator
    ThousandsSeparator = 0xB2,

    /// Decimal separator
    DecimalSeparator = 0xB3,

    /// Currency unit key
    CurrencyUnit = 0xB4,

    /// Currency sub-unit
    CurrencySubUnit = 0xB5,

    /// Keypad (
    KeypadOpenParen = 0xB6,

    /// Keypad )
    KeypadCloseParen = 0xB7,

    /// Keypad bracket open
    KeypadOpenBracket = 0xB8,

    /// Keypad bracket close
    KeypadCloseBraket = 0xB9,

    /// Keypad tab
    KeypadTab = 0xBA,

    /// Keypad backspace
    KeypadBackspace = 0xBB,

    /// Keypad A key
    KeypadA = 0xBC,

    /// Keypad B key
    KeypadB = 0xBD,

    /// Keypad C key
    KeypadC = 0xBE,

    /// Keypad D key
    KeypadD = 0xBF,

    /// Keypad E key
    KeypadE = 0xC0,

    /// Keypad F key
    KeypadF = 0xC1,

    /// Keypad XOR
    KeypadXOR = 0xC2,

    /// Keypad ^ key
    KeypadCircumflex = 0xC3,

    /// Keypad % key
    KeypadPercent = 0xC4,

    /// Keypad < key
    KeypadLt = 0xC5,

    /// Keypad > key
    KeypadGt = 0xC6,

    /// Keypad &
    KeypadAmpersand = 0xC7,

    /// Keypad && key
    KeypadAnd = 0xC8,

    /// Keypad |
    KeypadVerticalBar = 0xC9,

    /// Keypad ||
    KeypadOr = 0xCA,

    /// Keypad :
    KeypadColon = 0xCB,

    /// Keypad hash key
    KeypadHash = 0xCC,

    /// Keypad space
    KeypadSpace = 0xCD,

    /// Keypad at key
    KeypadAt = 0xCE,

    /// Keypad exclamation key
    KeypadExclamation = 0xCF,

    /// Keypad memory store
    KeypadMemoryStore = 0xD0,

    /// Keypad memory recall
    KeypadMemoryRecall = 0xD1,

    /// Keypad memory clear
    KeypadMemoryClear = 0xD2,

    /// Keypad memory add
    KeypadMemoryAdd = 0xD3,

    /// Keypad subtract
    KeypadMemorySubtract = 0xD4,

    /// Keypad multiply
    KeypadMemoryMultiply = 0xD5,

    /// Keypad memory divide
    KeypadMemoryDivide = 0xD6,

    /// Keypad +/- key
    KeypadPlusMinus = 0xD7,

    /// Keypad clear
    KeypadClear = 0xD8,

    /// Keypad clear entry
    KeypadClearEntry = 0xD9,

    /// Keypad binary
    KeypadBinary = 0xDA,

    /// Keypad octal
    KeypadOctal = 0xDB,

    /// Keypad decimal
    KeypadDecimal = 0xDC,

    /// Keypad hexadecimal
    KeypadHexadecimal = 0xDD,
    /*
    E0 Keyboard LeftControl DV 58 ✓ ✓ ✓ 4/101/104
    E1 Keyboard LeftShift DV 44 ✓ ✓ ✓ 4/101/104
    E2 Keyboard LeftAlt DV 60 ✓ ✓ ✓ 4/101/104
    E3 Keyboard Left GUI11,33 DV 127 ✓ ✓ ✓ 104
    E4 Keyboard RightControl DV 64 ✓ ✓ ✓ 101/104
    87
    Usage ID Usage Name Usage Type AT-101 PC-AT Mac Unix Boot
    E5 Keyboard RightShift DV 57 ✓ ✓ ✓ 4/101/104
    E6 Keyboard RightAlt DV 62 ✓ ✓ ✓ 101/104
    E7 Keyboard Right GUI11,34*/
}
