use core::mem::MaybeUninit;

use seq_macro::seq;
use usb_device::{bus::UsbBus, class::UsbClass, device::UsbDevice};

/**
 * Represents a set of USB endpoints that can be polled together.
 */
pub trait UsbFeatureSet<B: UsbBus> {
    type TPoll;
    fn poll_all(&mut self, device: &mut UsbDevice<B>) -> Option<Self::TPoll>;
}

/**
 * Represents something that a USB device is able to do. A feature may take
 * ownership of one or more USB endpoints and define the logic that needs to be
 * done with them. A USB device, in the context of dxkb, may be defined as a
 * collection of USB features that may be polled together.
 */
pub trait UsbFeature<B> {
    const EP: usize;
    type TPoll;

    fn poll(&mut self) -> Self::TPoll;
    fn endpoints_mut(&mut self) -> [&mut dyn UsbClass<B>; Self::EP];
}

/*
 * seq_macro::seq!(N in 0..$npins {
     match col {
         #(
             N => self.N.set_state(value),
         )*
         _ => panic!("Attempt to set state of a row pin out of bounds!")
     }
 });
 */

 // impl<B: UsbBus, $($x: UsbFeature<B>),*> EndpointSet<B> for ($($x,)*) where [(); 0 $(+ $x::EP)*]: {
 //     fn poll_all(&mut self, device: &mut UsbDevice<B>) {
 //         let mut eps: [MaybeUninit<&mut dyn UsbClass<B>>; 0 $(+ $x::EP)*] = MaybeUninit::uninit().transpose();
 //         let mut i = 0;
 //         $(
 //             let eps_x = self.${index()}.endpoints_mut();
 //             eps_x.into_iter().for_each(|ep| {
 //                 eps[i].write(ep);
 //                 i += 1;
 //             });
 //         )*

 //         let eps = unsafe {
 //             // SAFETY: number of elements written to eps must be 0 + sum of EPs of all features, so the array is fully initialized.
 //             eps.assume_init_mut()
 //         };

 //         if device.poll(eps) {
 //             $(self.${index()}.poll();)*
 //         }
 //     }
 // }

 macro_rules! endpoint_set_impl {
     ($($x:ident)*) => {
         impl<B: UsbBus, $($x: UsbFeature<B>),*> UsbFeatureSet<B> for ($(&mut $x,)*) where [(); 0 $(+ $x::EP)*]: {
             type TPoll = ($($x::TPoll,)*);

             fn poll_all(&mut self, device: &mut UsbDevice<B>) -> Option<Self::TPoll> {
                 let mut eps: [MaybeUninit<&mut dyn UsbClass<B>>; 0 $(+ $x::EP)*] = MaybeUninit::uninit().transpose();
                 let mut i = 0;
                 $(
                     let $x = self.${index()}.endpoints_mut();
                     $x.into_iter().for_each(|ep| {
                         eps[i].write(ep);
                         i += 1;
                     });
                 )*

                 let eps = unsafe {
                     // SAFETY: number of elements written to eps must be 0 + sum of EPs of all features, so the array is fully initialized.
                     eps.assume_init_mut()
                 };

                 if device.poll(eps) {
                     Some(
                        (
                         $(
                             {
                                 let $x = 0; // Dummy variable to be able to use metavars.
                                 self.${index()}.poll()
                             }
                         ),*
                        )
                     )
                 } else {
                     None
                 }
             }
         }
     };

     ($n:literal) => {
         seq_macro::seq!(i in 0..$n {
             endpoint_set_impl!(#(_~i)*);
         });

     };
 }

endpoint_set_impl!(2);
endpoint_set_impl!(3);
endpoint_set_impl!(4);
