# dxkb

## What is it?

dxkb is a keyboard firmware library purely developed in Rust, that provides the
functionality to build your own keyboard. It is mainly designed for my personal
use and for learning purposes. As of today, it only provides support for
building split keyboards on the STM32F411 microcontroller.

## What is it not?

dxkb is **not** a keyboard firmware that is provided ready to be flashed into a
MCU. The API that provides is designed considering that the final user will
define its own hardware configuration, main and ISR functions, as well as any
other functionality aside from the keyboard itself, that is not included on the
library (e.g. LEDs, OLED displays etc.)

## Design tenets

As a learning project, I prioritize the following principles during the development:
 - **Types first**: Most of the keyboard configuration happens through types, that
   defines the configuration of the firmware in compile time, without adding
   extra overhead, or possible errors in runtime. This library makes a heavy use
   of types for defining a lot of aspects of the device, such as the size of the
   key matrix, debouncing algorithms, etc. This also usually translate into a
   heavy use of macros, that help to define types and impls that are useful to
   achieve this purpose.
 
 - **No hardware abstraction frameworks**: As part of the learning process in
   embedded development, it is preferred to avoid the usage of frameworks that
   abstracts too much over the hardware, and working close to it instead.
   Usage of these kind of frameworks, like Embassy or RTIC is not contemplated
   as for now.
 
 - **Rust unstable**: Nightly versions of Rust are preferred to play with all the
   unstable Rust features. The main driver for using Rust unstable is the
   ability to use generic_const_exprs, that are just too cool to not use them,
   even though it is still an incomplete feature. A successful compilation is
   not guaranteed in any other Rust version than the specified by this
   repository.
 
 - **Some unsafe is fine**: For the sake of avoiding as much as possible using
   unwraps or useless error handling when some condition cannot be proven to the
   compiler in compile time, and add extra overhead to the final binary; using
   some unsafe code is fine from the perspective of this library. For example,
   there are places in this library where some UnsafeCell's are used instead of
   RefCells, to avoid the overhead of the RefCell's borrow checking in,
   especially considering the fact that the only supported MCU is single-core.
 
 - **Low number of external dependencies**: As a learning project, it is
   preferred to investigate and develop my own version of some components,
   instead of using already existing libraries that can fit the needs. Base
   libraries such as HAL, PACs and USB control libraries are used, but
   generally, building the functionality as part of the library is preferred.

## Features

 - Support for split keyboards (actually, it does not support anything else but
   split keyboards).
 
 - Key matrix:
   - Support for column to row and row to column scans.
   - Active-low press detection.
   - Support for key debouncing (Right now, only implemented an
     eager, per-key debouncing algorithm, see [QMK
     Docs](https://docs.qmk.fm/feature_debounce_type) for more information).
   - Support for input pins oversampling (Read multiple times the matrix input
     pins to ensure that the received signals are coming from presses and are
     not electrical noise).
   
 - Support for report keyboard HID protocol, that is NKRO by default. 
   Dual Boot + Report protocol support is still not implemented.
 
 - Debug endpoint that supports logging through a HID interface and debug
   command sending, such as commands to entering into the bootloader. Especially
   useful for PCBs that don't expose the STM32 debugging pins.
 
 - DMA-based serial communication across the two sides of the keyboard, up to 2
   MBaud with option for Full duplex and Half duplex, and automatic frame
   separation through the detection of IDLE signals in the line.
 
 - Split link: a low-footprint protocol built on top of serial that allows a
   reliable, bidirectional communication across the sides of the keyboard. This
   protocol combines capabilities from a link and transport protocols, having
   the ability to detect whether the link is up, negotiate the link
   establishment, and frame management, ensuring that the messages arrive to the
   other side in order, with no duplicates, and automatically retransmitting
   dropped frames if the peer couldn't confirm the reception of one.
 
 - Automatic USB master side detection and promotion.
 
 - Custom key definition: New keys can be built with ease on top of the default
   ones that can define their own logic when pressed or unpressed.

 - Multi layer, tree based layer definition: The keyboard supports the
   definition of multiple layers, that can be defined in a tree hierarchy
   defined through macros. In compile time, this layer hierarchy is flattened,
   so that there's no a performance penalty when leading with bigger layer
   trees.
 
 - Remote wakeup support: The keyboard may wake up their host when this latter
   one is suspended, after pressing a key.
 
 ## Examples
 
  - [Testing keyboard used for development purposes.](https://github.com/devcexx/dxkb/blob/master/crates/dxkb-main/src/targets/testkb_3x5/main.rs)
  - [My personal Lily58 keyboard firmware](https://github.com/devcexx/dxkb/tree/master/crates/dxkb-lily58l-stemcell), that uses [STeMCell](https://github.com/devcexx/STeMCell) as a drop-in replacement for the Arduino ProMicro, using a STM32F411 instead.
