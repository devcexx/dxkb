#[macro_export]
macro_rules! dev_error {
    () => {};
    ($($arg:tt)*) => {
        #[cfg(feature = "dev-log")]
        log::error!($($arg)*);
    }
}

#[macro_export]
macro_rules! dev_info {
    () => {};
    ($($arg:tt)*) => {
        #[cfg(feature = "dev-log")]
        log::info!($($arg)*);
    }
}

#[macro_export]
macro_rules! dev_warn {
    () => {};
    ($($arg:tt)*) => {
        #[cfg(feature = "dev-log")]
        log::warn!($($arg)*);
    }
}

#[macro_export]
macro_rules! dev_debug {
    () => {};
    ($($arg:tt)*) => {
        #[cfg(feature = "dev-log")]
        log::debug!($($arg)*);
    }
}
