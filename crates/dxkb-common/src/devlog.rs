#[macro_export]
#[cfg(feature = "__dev_log_enable_level_error")]
macro_rules! dev_error {
    () => {};
    ($($arg:tt)*) => {
        $crate::__log::error!($($arg)*);
    }
}

#[macro_export]
#[cfg(not(feature = "__dev_log_enable_level_error"))]
macro_rules! dev_error {
    () => {};
    ($($arg:tt)*) => {};
}

#[macro_export]
#[cfg(feature = "__dev_log_enable_level_info")]
macro_rules! dev_info {
    () => {};
    ($($arg:tt)*) => {
        $crate::__log::info!($($arg)*);
    }
}

#[macro_export]
#[cfg(not(feature = "__dev_log_enable_level_info"))]
macro_rules! dev_info {
    () => {};
    ($($arg:tt)*) => {};
}

#[macro_export]
#[cfg(feature = "__dev_log_enable_level_warn")]
macro_rules! dev_warn {
    () => {};
    ($($arg:tt)*) => {
        $crate::__log::warn!($($arg)*);
    }
}

#[macro_export]
#[cfg(not(feature = "__dev_log_enable_level_warn"))]
macro_rules! dev_warn {
    () => {};
    ($($arg:tt)*) => {};
}

#[macro_export]
#[cfg(feature = "__dev_log_enable_level_debug")]
macro_rules! dev_debug {
    () => {};
    ($($arg:tt)*) => {
        $crate::__log::debug!($($arg)*);
    }
}

#[macro_export]
#[cfg(not(feature = "__dev_log_enable_level_debug"))]
macro_rules! dev_debug {
    () => {};
    ($($arg:tt)*) => {};
}

#[macro_export]
#[cfg(feature = "__dev_log_enable_level_trace")]
macro_rules! dev_trace {
    () => {};
    ($($arg:tt)*) => {
        $crate::__log::trace!($($arg)*);
    }
}

#[macro_export]
#[cfg(not(feature = "__dev_log_enable_level_trace"))]
macro_rules! dev_trace {
    () => {};
    ($($arg:tt)*) => {};
}
