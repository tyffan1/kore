#[cfg(test)]
mod display_list_tests {
    use crate::display_list::{
        ClipRect, Color, DisplayCommand, DisplayList, DrawImage, DrawRect, DrawText,
    };

    #[test]
    fn empty_display_list() {
        let list = DisplayList::new();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn push_rect_increments_len() {
        let mut list = DisplayList::new();
        list.push_rect(DrawRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
            color: Color::BLACK,
            opacity: 1.0,
            translate: (0.0, 0.0),
        });
        assert_eq!(list.len(), 1);
        assert!(!list.is_empty());
    }

    #[test]
    fn command_ordering_preserved() {
        let mut list = DisplayList::new();
        list.push_rect(DrawRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            color: Color::BLACK,
            opacity: 1.0,
            translate: (0.0, 0.0),
        });
        list.push_text(DrawText {
            x: 5.0,
            y: 5.0,
            text: "hello".into(),
            font_size: 12.0,
            color: Color::WHITE,
            font_family: None,
            bold: false,
            italic: false,
            opacity: 1.0,
            translate: (0.0, 0.0),
        });
        list.push_image(DrawImage {
            x: 20.0,
            y: 20.0,
            width: 64.0,
            height: 64.0,
            atlas_id: 0,
        });

        let cmds = list.commands();
        assert_eq!(cmds.len(), 3);
        assert!(matches!(cmds[0], DisplayCommand::Rect(_)));
        assert!(matches!(cmds[1], DisplayCommand::Text(_)));
        assert!(matches!(cmds[2], DisplayCommand::Image(_)));
    }

    #[test]
    fn clip_push_pop_ordering() {
        let mut list = DisplayList::new();
        let clip = ClipRect { x: 0.0, y: 0.0, width: 200.0, height: 200.0 };
        list.push_clip(clip);
        list.push_rect(DrawRect {
            x: 10.0,
            y: 10.0,
            width: 50.0,
            height: 50.0,
            color: Color::WHITE,
            opacity: 1.0,
            translate: (0.0, 0.0),
        });
        list.pop_clip();

        let cmds = list.commands();
        assert_eq!(cmds.len(), 3);
        assert!(matches!(cmds[0], DisplayCommand::PushClip(_)));
        assert!(matches!(cmds[1], DisplayCommand::Rect(_)));
        assert!(matches!(cmds[2], DisplayCommand::PopClip));
    }

    #[test]
    fn clear_resets_list() {
        let mut list = DisplayList::new();
        list.push_rect(DrawRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            color: Color::BLACK,
            opacity: 1.0,
            translate: (0.0, 0.0),
        });
        list.clear();
        assert!(list.is_empty());
    }

    #[test]
    fn clip_rect_contains() {
        let clip = ClipRect { x: 10.0, y: 10.0, width: 100.0, height: 100.0 };
        assert!(clip.contains(50.0, 50.0));
        assert!(clip.contains(10.0, 10.0));
        assert!(!clip.contains(5.0, 5.0));
        assert!(!clip.contains(200.0, 200.0));
    }

    #[test]
    fn clip_rect_intersects() {
        let a = ClipRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };
        let b = ClipRect { x: 50.0, y: 50.0, width: 100.0, height: 100.0 };
        let c = ClipRect { x: 200.0, y: 200.0, width: 50.0, height: 50.0 };
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn color_from_rgba8() {
        let c = Color::from_rgba8(255, 128, 0, 255);
        assert!((c.r - 1.0).abs() < 1e-3);
        assert!((c.g - 0.502).abs() < 1e-2);
        assert!((c.b - 0.0).abs() < 1e-3);
        assert!((c.a - 1.0).abs() < 1e-3);
    }

    #[test]
    fn multiple_clips_stack() {
        let mut list = DisplayList::new();
        let outer = ClipRect { x: 0.0, y: 0.0, width: 500.0, height: 500.0 };
        let inner = ClipRect { x: 10.0, y: 10.0, width: 100.0, height: 100.0 };
        list.push_clip(outer);
        list.push_clip(inner);
        list.pop_clip();
        list.pop_clip();

        let cmds = list.commands();
        assert_eq!(cmds.len(), 4);
        assert!(matches!(cmds[0], DisplayCommand::PushClip(_)));
        assert!(matches!(cmds[1], DisplayCommand::PushClip(_)));
        assert!(matches!(cmds[2], DisplayCommand::PopClip));
        assert!(matches!(cmds[3], DisplayCommand::PopClip));
    }
}
