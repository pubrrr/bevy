use crate::{CalculatedClip, Node};
use bevy_core::FloatOrd;
use bevy_ecs::{
    entity::Entity,
    prelude::Component,
    reflect::ReflectComponent,
    system::{Query, Res, Resource},
};
use bevy_input::{mouse::MouseButton, touch::Touches, Input};
use bevy_math::Vec2;
use bevy_reflect::{Reflect, ReflectDeserialize};
use bevy_transform::components::GlobalTransform;
use bevy_window::Windows;
use serde::{Deserialize, Serialize};

/// Describes what type of input interaction has occurred for a UI node.
///
/// This is commonly queried with a `Changed<Interaction>` filter.
#[derive(Component, Copy, Clone, Eq, PartialEq, Debug, Reflect, Serialize, Deserialize)]
#[reflect_value(Component, Serialize, Deserialize, PartialEq)]
pub enum Interaction {
    /// The node has been clicked
    Clicked,
    /// The node has been hovered over
    Hovered,
    /// Nothing has happened
    None,
}

impl Default for Interaction {
    fn default() -> Self {
        Interaction::None
    }
}

/// Describes whether the node should block interactions with lower nodes
#[derive(Component, Copy, Clone, Eq, PartialEq, Debug, Reflect, Serialize, Deserialize)]
#[reflect_value(Component, Serialize, Deserialize, PartialEq)]
pub enum FocusPolicy {
    /// Blocks interaction
    Block,
    /// Lets interaction pass through
    Pass,
}

impl Default for FocusPolicy {
    fn default() -> Self {
        FocusPolicy::Block
    }
}

pub type NodeQuery<'a> = (
    &'a Node,
    &'a GlobalTransform,
    &'a mut Interaction,
    Option<&'a FocusPolicy>,
    Option<&'a CalculatedClip>,
);

/// The system that sets Interaction for all UI elements based on the mouse cursor activity
#[allow(clippy::type_complexity)]
pub fn ui_focus_system(
    windows: Res<Windows>,
    mouse_button_input: Res<Input<MouseButton>>,
    touches_input: Res<Touches>,
    node_query: Query<NodeQuery>,
) {
    focus_ui(windows, mouse_button_input, touches_input, node_query)
}

#[allow(clippy::type_complexity)]
fn focus_ui<Cursor: CursorResource>(
    windows: Res<Cursor>,
    mouse_button_input: Res<Input<MouseButton>>,
    touches_input: Res<Touches>,
    mut node_query: Query<NodeQuery>,
) {
    set_all_interactions_to_none(&mut node_query);

    let cursor_position = match windows.get_cursor_position() {
        None => return,
        Some(cursor_position) => cursor_position,
    };

    let mut moused_over_z_sorted_nodes = node_query
        .iter_mut()
        .filter_map(
            |(node, global_transform, interaction, focus_policy, clip)| {
                let position = global_transform.translation;
                let ui_position = position.truncate();
                let extents = node.size / 2.0;
                let mut min = ui_position - extents;
                let mut max = ui_position + extents;
                if let Some(clip) = clip {
                    min = Vec2::max(min, clip.clip.min);
                    max = Vec2::min(max, clip.clip.max);
                }

                let contains_cursor = (min.x..max.x).contains(&cursor_position.x)
                    && (min.y..max.y).contains(&cursor_position.y);

                if contains_cursor {
                    Some((focus_policy, interaction, FloatOrd(position.z)))
                } else {
                    None
                }
            },
        )
        .collect::<Vec<_>>();

    moused_over_z_sorted_nodes.sort_by_key(|(_, _, z)| -*z);

    let mouse_clicked = mouse_button_input.just_pressed(MouseButton::Left)
        || mouse_button_input.pressed(MouseButton::Left)
        || touches_input.just_released(0);

    for (focus_policy, mut interaction, _) in moused_over_z_sorted_nodes {
        if mouse_clicked {
            *interaction = Interaction::Clicked;
        } else {
            *interaction = Interaction::Hovered;
        }

        match focus_policy.cloned().unwrap_or(FocusPolicy::Block) {
            FocusPolicy::Block => {
                break;
            }
            FocusPolicy::Pass => { /* allow the next node to be hovered/clicked */ }
        }
    }
}

fn set_all_interactions_to_none(node_query: &mut Query<NodeQuery>) {
    for (_node, _global_transform, mut interaction, _focus_policy, _clip) in node_query.iter_mut() {
        *interaction = Interaction::None;
    }
}

trait CursorResource: Resource {
    fn get_cursor_position(&self) -> Option<Vec2>;
}

impl CursorResource for Windows {
    fn get_cursor_position(&self) -> Option<Vec2> {
        self.get_primary()
            .and_then(|window| window.cursor_position())
    }
}

#[cfg(test)]
mod tests {
    use rstest::*;

    use bevy_app::App;
    use bevy_ecs::event::Events;
    use bevy_ecs::prelude::ParallelSystemDescriptorCoercion;
    use bevy_input::touch::{touch_screen_input_system, TouchInput, TouchPhase};
    use bevy_math::Vec3;

    use super::*;

    const NODE_SIZE: f32 = 5.0;

    #[rstest]
    #[case::no_cursor(vec![(None, Interaction::None)])]
    #[case::not_hovered(vec![(Some((0., 0.)), Interaction::None)])]
    #[case::hovered(vec![(Some((10., 10.)), Interaction::Hovered)])]
    #[case::hovered_then_not_hovered(vec![
        (Some((10., 10.)), Interaction::Hovered),
        (Some((0., 0.)), Interaction::None),
    ])]
    #[case::hovered_then_no_cursor(vec![
        (Some((10., 10.)), Interaction::Hovered),
        (None, Interaction::None),
    ])]
    fn hovered(#[case] test_set: Vec<(Option<(f32, f32)>, Interaction)>) {
        let mut app = TestApp::new();
        let entity = app.spawn_node_entity_at(10., 10.);

        for (cursor_position, expected_interaction) in test_set {
            app.set_cursor_position(cursor_position);

            app.run_step();

            let interaction = app.get_interaction(entity);
            assert_eq!(
                &expected_interaction, interaction,
                "for position {:?}",
                cursor_position,
            );
        }
    }

    #[rstest]
    #[case::mouse_no_cursor(vec![(None, Interaction::None)], false)]
    #[case::mouse_not_hovered(vec![(Some((0., 0.)), Interaction::None)], false)]
    #[case::mouse_clicked(vec![(Some((10., 10.)), Interaction::Clicked)], false)]
    #[case::mouse_clicked_then_not_hovered(vec![
        (Some((10., 10.)), Interaction::Clicked),
        (Some((0., 0.)), Interaction::None),
    ], false)]
    #[case::mouse_clicked_then_no_cursor(vec![
        (Some((10., 10.)), Interaction::Clicked),
        (None, Interaction::None),
    ], false)]
    #[case::touch_no_cursor(vec![(None, Interaction::None)], true)]
    #[case::touch_not_hovered(vec![(Some((0., 0.)), Interaction::None)], true)]
    #[case::touch_clicked(vec![(Some((10., 10.)), Interaction::Clicked)], true)]
    #[case::touch_clicked_then_not_hovered(vec![
        (Some((10., 10.)), Interaction::Clicked),
        (Some((0., 0.)), Interaction::None),
    ], true)]
    #[case::touch_clicked_then_no_cursor(vec![
        (Some((10., 10.)), Interaction::Clicked),
        (None, Interaction::None),
    ], true)]
    fn clicked(#[case] test_set: Vec<(Option<(f32, f32)>, Interaction)>, #[case] touch: bool) {
        let mut app = TestApp::new();
        let entity = app.spawn_node_entity_at(10., 10.);

        for (cursor_position, expected_interaction) in test_set {
            app.set_cursor_position(cursor_position);
            if touch {
                app.set_screen_touched();
            } else {
                app.set_mouse_clicked();
            }

            app.run_step();

            let interaction = app.get_interaction(entity);
            assert_eq!(
                &expected_interaction, interaction,
                "for position {:?}",
                cursor_position,
            );
        }
    }

    #[rstest]
    #[case::no_focus_policy(None, Interaction::None)]
    #[case::focus_policy_block(Some(FocusPolicy::Block), Interaction::None)]
    #[case::focus_policy_pass(Some(FocusPolicy::Pass), Interaction::Clicked)]
    fn click_stacked_nodes(
        #[case] focus_policy: Option<FocusPolicy>,
        #[case] expected_interaction: Interaction,
    ) {
        let mut app = TestApp::new();
        let background_entity = app.spawn_node_entity_with_z_at(10., 10., 0., focus_policy);
        let foreground_entity = app.spawn_node_entity_with_z_at(10., 10., 5., focus_policy);

        app.set_cursor_position(Some((10., 10.)));
        app.set_mouse_clicked();

        app.run_step();

        assert_eq!(
            &Interaction::Clicked,
            app.get_interaction(foreground_entity)
        );
        assert_eq!(
            &expected_interaction,
            app.get_interaction(background_entity)
        );
    }

    #[test]
    fn hover_one_node_then_click_the_other_where_both_overlap() {
        let mut app = TestApp::new();
        let background_node_position = 8.;
        let background_entity = app.spawn_node_entity_with_z_at(
            background_node_position,
            background_node_position,
            0.,
            Some(FocusPolicy::Block),
        );
        let foreground_entity =
            app.spawn_node_entity_with_z_at(10., 10., 5., Some(FocusPolicy::Block));

        app.set_cursor_position(Some((6., 6.)));

        app.run_step();

        assert_eq!(&Interaction::None, app.get_interaction(foreground_entity));
        assert_eq!(
            &Interaction::Hovered,
            app.get_interaction(background_entity)
        );

        app.set_cursor_position(Some((background_node_position, background_node_position)));
        app.set_mouse_clicked();

        app.run_step();

        assert_eq!(
            &Interaction::Clicked,
            app.get_interaction(foreground_entity)
        );
        assert_eq!(&Interaction::None, app.get_interaction(background_entity));
    }

    #[test]
    fn click_then_move_away_and_release_mouse_button() {
        let mut app = TestApp::new();
        let entity = app.spawn_node_entity_at(10., 10.);

        app.set_cursor_position(Some((10., 10.)));
        app.set_mouse_clicked();

        app.run_step();
        assert_eq!(&Interaction::Clicked, app.get_interaction(entity));

        app.set_cursor_position(Some((0., 0.)));

        app.run_step();
        assert_eq!(&Interaction::None, app.get_interaction(entity));

        app.set_mouse_released();

        app.run_step();
        assert_eq!(&Interaction::None, app.get_interaction(entity));
    }

    #[test]
    fn click_and_keep_pressed() {
        let mut app = TestApp::new();
        let entity = app.spawn_node_entity_at(10., 10.);
        app.set_cursor_position(Some((10., 10.)));
        app.set_mouse_clicked();

        app.run_step();
        assert_eq!(&Interaction::Clicked, app.get_interaction(entity));

        app.run_step();
        assert_eq!(&Interaction::Clicked, app.get_interaction(entity));
    }

    #[test]
    fn click_and_release() {
        let mut app = TestApp::new();
        let entity = app.spawn_node_entity_at(10., 10.);
        app.set_cursor_position(Some((10., 10.)));

        app.set_mouse_clicked();
        app.run_step();
        assert_eq!(&Interaction::Clicked, app.get_interaction(entity));

        app.set_mouse_released();
        app.run_step();
        assert_eq!(&Interaction::Hovered, app.get_interaction(entity));
    }

    #[test]
    fn click_and_release_in_single_frame() {
        let mut app = TestApp::new();
        let entity = app.spawn_node_entity_at(10., 10.);
        app.set_cursor_position(Some((10., 10.)));

        app.set_mouse_clicked();
        app.set_mouse_released();
        app.run_step();
        assert_eq!(&Interaction::Clicked, app.get_interaction(entity));

        app.run_step();
        assert_eq!(&Interaction::Hovered, app.get_interaction(entity));
    }

    struct TestApp {
        app: App,
    }

    impl TestApp {
        fn new() -> TestApp {
            let mut app = App::new();
            app.init_resource::<Input<MouseButton>>()
                .init_resource::<Touches>()
                .add_event::<TouchInput>()
                .add_system(focus_ui::<WindowsDouble>.label("under_test"))
                .add_system(touch_screen_input_system.before("under_test"));

            TestApp { app }
        }

        fn set_cursor_position(&mut self, cursor_position: Option<(f32, f32)>) {
            let cursor_position = cursor_position.map(|(x, y)| Vec2::new(x, y));
            self.app.insert_resource(WindowsDouble { cursor_position });
        }

        fn set_screen_touched(&mut self) {
            self.app
                .world
                .get_resource_mut::<Events<TouchInput>>()
                .unwrap()
                .send(TouchInput {
                    phase: TouchPhase::Ended,
                    position: Default::default(),
                    force: None,
                    id: 0,
                })
        }

        fn set_mouse_clicked(&mut self) {
            let mut mouse_input = self
                .app
                .world
                .get_resource_mut::<Input<MouseButton>>()
                .unwrap();
            mouse_input.press(MouseButton::Left);
        }

        fn set_mouse_released(&mut self) {
            let mut mouse_input = self
                .app
                .world
                .get_resource_mut::<Input<MouseButton>>()
                .unwrap();
            mouse_input.release(MouseButton::Left);
        }

        fn spawn_node_entity_at(&mut self, x: f32, y: f32) -> Entity {
            self.app
                .world
                .spawn()
                .insert(GlobalTransform {
                    translation: Vec3::new(x, y, 0.0),
                    ..GlobalTransform::default()
                })
                .insert(Node {
                    size: Vec2::new(NODE_SIZE, NODE_SIZE),
                })
                .insert(Interaction::None)
                .id()
        }

        fn spawn_node_entity_with_z_at(
            &mut self,
            x: f32,
            y: f32,
            z: f32,
            focus_policy: Option<FocusPolicy>,
        ) -> Entity {
            let mut entity = self.app.world.spawn();
            if let Some(focus_policy) = focus_policy {
                entity.insert(focus_policy);
            }

            entity
                .insert(GlobalTransform {
                    translation: Vec3::new(x, y, z),
                    ..GlobalTransform::default()
                })
                .insert(Node {
                    size: Vec2::new(NODE_SIZE, NODE_SIZE),
                })
                .insert(Interaction::None)
                .id()
        }

        fn run_step(&mut self) {
            self.app.schedule.run_once(&mut self.app.world);

            let mut mouse_input = self
                .app
                .world
                .get_resource_mut::<Input<MouseButton>>()
                .unwrap();
            mouse_input.clear();
        }

        fn get_interaction(&self, entity: Entity) -> &Interaction {
            &self.app.world.get::<Interaction>(entity).unwrap()
        }
    }

    #[derive(Debug, Default)]
    struct WindowsDouble {
        cursor_position: Option<Vec2>,
    }

    impl CursorResource for WindowsDouble {
        fn get_cursor_position(&self) -> Option<Vec2> {
            self.cursor_position
        }
    }
}
