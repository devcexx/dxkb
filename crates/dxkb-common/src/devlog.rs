
#[macro_export]
#[cfg(feature = "dev-log")]
macro_rules! dev_error {
    () => {};
    ($($arg:tt)*) => {
        $crate::__log::error!($($arg)*);
    }
}

#[macro_export]
#[cfg(not(feature = "dev-log"))]
macro_rules! dev_error {
    () => {};
    ($($arg:tt)*) => {}
}

#[macro_export]
#[cfg(feature = "dev-log")]
macro_rules! dev_info {
    () => {};
    ($($arg:tt)*) => {
        $crate::__log::info!($($arg)*);
    }
}

#[macro_export]
#[cfg(not(feature = "dev-log"))]
macro_rules! dev_info {
    () => {};
    ($($arg:tt)*) => {}
}

#[macro_export]
#[cfg(feature = "dev-log")]
macro_rules! dev_warn {
    () => {};
    ($($arg:tt)*) => {
        $crate::__log::warn!($($arg)*);
    }
}

#[macro_export]
#[cfg(not(feature = "dev-log"))]
macro_rules! dev_warn {
    () => {};
    ($($arg:tt)*) => {}
}

#[macro_export]
#[cfg(feature = "dev-log")]
macro_rules! dev_debug {
    () => {};
    ($($arg:tt)*) => {
        $crate::__log::debug!($($arg)*);
    }
}

#[macro_export]
#[cfg(not(feature = "dev-log"))]
macro_rules! dev_debug {
    () => {};
    ($($arg:tt)*) => {}
}
