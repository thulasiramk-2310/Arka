//! A widget that paints whatever its `paint` closure adds to a GTK snapshot. The frosted glass
//! behind the desktop's cards and buttons is drawn this way: as textures and clips that the GPU
//! puts together, instead of a full-screen picture redrawn by the processor on every frame of
//! an animation (which made the ring and the menu stutter on a laptop).

use std::cell::RefCell;

use gtk::subclass::prelude::*;
use gtk::{glib, prelude::*};
use gtk4 as gtk;

pub type Paint = Box<dyn Fn(&gtk::Widget, &gtk::Snapshot)>;

#[derive(Default)]
pub struct Imp {
    paint: RefCell<Option<Paint>>,
}

#[glib::object_subclass]
impl ObjectSubclass for Imp {
    const NAME: &'static str = "ArkaGlass";
    type Type = Glass;
    type ParentType = gtk::Widget;
}

impl ObjectImpl for Imp {}

impl WidgetImpl for Imp {
    fn snapshot(&self, snapshot: &gtk::Snapshot) {
        if let Some(paint) = self.paint.borrow().as_ref() {
            paint(self.obj().upcast_ref(), snapshot);
        }
    }
}

glib::wrapper! {
    pub struct Glass(ObjectSubclass<Imp>) @extends gtk::Widget;
}

impl Glass {
    pub fn new(paint: Paint) -> Self {
        let glass: Self = glib::Object::new();
        *glass.imp().paint.borrow_mut() = Some(paint);
        glass
    }
}
