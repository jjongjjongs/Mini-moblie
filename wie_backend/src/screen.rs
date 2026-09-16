use alloc::{boxed::Box, vec::Vec};

use bytemuck::pod_collect_to_vec;

use crate::{
    canvas::{Image, PixelType, Rgb8Pixel, Rgb332Pixel, Rgb565Pixel, VecImageBuffer},
    system::System,
};

use wie_util::Result;

pub trait Screen: Send + Sync {
    fn request_redraw(&self) -> Result<()>;
    fn paint(&self, image: &dyn Image);
    fn width(&self) -> u32;
    fn height(&self) -> u32;
}

/// Puts a finished frame on the host's screen, turned upright first if the
/// title draws it sideways.
///
/// Every path that presents a frame - the WIPI-C flush, the KTF engine's own
/// LCD buffer, the MIDP display - goes through here, so which of them a title
/// happens to use does not decide whether the turn is applied.
///
/// A title that is meant to be played with the handset held sideways composes
/// a landscape picture and copies it onto its upright panel a quarter turn
/// clockwise; nothing in the API says it did, so the fact is recorded per
/// title in `crate::quirks` and set on the system by its emulator. Turning the
/// frame back is the whole of the correction: the title still lays out for the
/// panel it was given, and only what the host shows changes.
pub fn present(system: &System, image: &dyn Image) {
    let platform = system.platform();
    let screen = platform.screen();

    if system.title_draws_sideways()
        && let Some(upright) = quarter_turn_left(image)
    {
        screen.paint(&*upright);
        return;
    }

    screen.paint(image);
}

/// A copy of `image` turned a quarter turn counter-clockwise, so a `w`x`h`
/// picture comes back `h`x`w`.
///
/// Pixels are moved as they are rather than through [`crate::canvas::Color`],
/// so nothing is lost on the way round. `None` when the image's own bytes do
/// not amount to the size it reports, which leaves the caller showing the
/// frame it already has rather than none.
pub fn quarter_turn_left(image: &dyn Image) -> Option<Box<dyn Image>> {
    match image.bytes_per_pixel() {
        1 => turn_left::<Rgb332Pixel>(image),
        2 => turn_left::<Rgb565Pixel>(image),
        4 => turn_left::<Rgb8Pixel>(image),
        _ => None,
    }
}

fn turn_left<T>(image: &dyn Image) -> Option<Box<dyn Image>>
where
    T: PixelType + 'static,
{
    let width = image.width() as usize;
    let height = image.height() as usize;
    if width == 0 || height == 0 {
        return None;
    }

    let source: Vec<T::DataType> = pod_collect_to_vec(&image.raw());
    if source.len() < width * height {
        return None;
    }

    // The turned picture is `height` wide, and the source's column `x` becomes
    // its row `width - 1 - x` - counter-clockwise, which is what undoes the
    // clockwise turn a sideways title made.
    let mut turned = source.clone();
    turned.truncate(width * height);
    for y in 0..height {
        for x in 0..width {
            turned[(width - 1 - x) * height + y] = source[y * width + x];
        }
    }

    Some(Box::new(VecImageBuffer::<T>::from_raw(height as u32, width as u32, turned)))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::quarter_turn_left;
    use crate::canvas::{Image, Rgb565Pixel, VecImageBuffer};

    /// Rows read bottom-up as columns, which is a quarter turn to the left.
    #[test]
    fn a_frame_comes_back_turned_counter_clockwise() {
        //  1 2 3        3 6
        //  4 5 6   ->   2 5
        //                1 4
        let image = VecImageBuffer::<Rgb565Pixel>::from_raw(3, 2, vec![1, 2, 3, 4, 5, 6]);

        let turned = quarter_turn_left(&image).unwrap();

        assert_eq!((turned.width(), turned.height()), (2, 3));
        assert_eq!(turned.raw().as_ref(), bytemuck::cast_slice(&[3u16, 6, 2, 5, 1, 4]));
    }

    /// Two turns are a half turn, four are none at all, so nothing is lost or
    /// transposed on the way round.
    #[test]
    fn four_turns_are_the_frame_it_started_as() {
        let image = VecImageBuffer::<Rgb565Pixel>::from_raw(4, 2, (0..8u16).collect());

        let mut turned = quarter_turn_left(&image).unwrap();
        for _ in 0..3 {
            turned = quarter_turn_left(&*turned).unwrap();
        }

        assert_eq!((turned.width(), turned.height()), (4, 2));
        assert_eq!(turned.raw().as_ref(), image.raw().as_ref());
    }

    /// An image whose bytes do not amount to its size is not turned at all,
    /// rather than turned from whatever happens to follow it.
    #[test]
    fn a_frame_shorter_than_it_claims_is_left_alone() {
        let image = VecImageBuffer::<Rgb565Pixel>::from_raw(4, 4, vec![1, 2, 3, 4]);

        assert!(quarter_turn_left(&image).is_none());
    }
}
