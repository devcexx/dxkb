use usb_device::{bus::UsbBus, device::UsbDevice};

pub trait UsbRemoteWakeup {
    /**
     * Initiates a remote signaling to wake up the USB host. As per USB
     * specification, the signalling must last between 1 ms and 15 ms. The
     * caller is responsible of calling [`remote_wakeup_end`] after the required
     * time has elapsed.
     */
    fn remote_wakeup_start(&mut self);

    /**
     * Ends the remote wakeup signaling. Must be called after a call to
     * [`remote_wakeup_start`] and after the required time has elapsed.
     */
    fn remote_wakeup_end(&mut self);
}

#[cfg(feature = "stm32f411")]
impl<'a, B: UsbBus> UsbRemoteWakeup for UsbDevice<'a, B> {
    fn remote_wakeup_start(&mut self) {
        use stm32f4xx_hal::pac::OTG_FS_DEVICE;
        unsafe {
            OTG_FS_DEVICE::steal().dctl().modify(|_, w| {
                w.rwusig().set_bit()
            });
        }
    }

    fn remote_wakeup_end(&mut self) {
        use stm32f4xx_hal::pac::OTG_FS_DEVICE;
        unsafe {
            OTG_FS_DEVICE::steal().dctl().modify(|_, w| {
                w.rwusig().clear_bit()
            });
        }
    }
}
