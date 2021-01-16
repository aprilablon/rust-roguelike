use bracket_lib::prelude::*;
use specs::prelude::*;

mod ai;
mod components;
#[allow(dead_code)]
mod constants;
mod damage_system;
mod gamelog;
mod generator;
mod inventory_system;
mod item_listing_system;
mod map;
mod map_indexing;
mod melee_system;
mod player;
mod rect;
mod ui;
mod visibility;

use ai::MonsterAI;
use components::*;
use constants::*;
use damage_system::DamageSystem;
use gamelog::GameLog;
use inventory_system::*;
use item_listing_system::ItemListingSystem;
use map::{draw_map, Map};
use map_indexing::MapIndexingSystem;
use melee_system::MeleeCombatSystem;
use player::player_input;
use rect::Rect;
use visibility::VisibilitySystem;

#[derive(PartialEq, Copy, Clone)]
pub enum RunState {
    AwaitingInput,
    PreRun,
    PlayerTurn,
    MonsterTurn,
    ShowInventory,
    Dead,
}

pub struct State {
    ecs: World,
}

impl State {
    fn run_systems(&mut self) {
        let mut vis = VisibilitySystem {};
        vis.run_now(&self.ecs);
        let mut mob = MonsterAI {};
        mob.run_now(&self.ecs);
        let mut map_index = MapIndexingSystem {};
        map_index.run_now(&self.ecs);
        let mut melee = MeleeCombatSystem {};
        melee.run_now(&self.ecs);
        let mut damage = DamageSystem {};
        damage.run_now(&self.ecs);
        let mut pickup = ItemCollectionSystem {};
        pickup.run_now(&self.ecs);
        let mut item_listing = ItemListingSystem {};
        item_listing.run_now(&self.ecs);
        let mut potions = PotionUseSystem {};
        potions.run_now(&self.ecs);

        self.ecs.maintain();
    }
}

impl GameState for State {
    fn tick(&mut self, ctx: &mut BTerm) {
        ctx.cls();

        draw_map(&self.ecs, ctx);

        {
            let positions = self.ecs.read_storage::<Position>();
            let renderables = self.ecs.read_storage::<Renderable>();
            let player = self.ecs.read_storage::<Player>();
            let map = self.ecs.fetch::<Map>();

            for (pos, render) in (&positions, &renderables).join() {
                if map.visible_tiles[pos.x as usize][pos.y as usize] {
                    ctx.set(pos.x, pos.y, render.fg, render.bg, render.glyph);
                }
            }

            // Re-render the player, since we want them to always be on top of objects
            for (pos, render, _player) in (&positions, &renderables, &player).join() {
                ctx.set(pos.x, pos.y, render.fg, render.bg, render.glyph);
            }

            ui::draw_ui(&self.ecs, ctx);
        }

        let mut new_runstate;
        {
            let runstate = self.ecs.fetch::<RunState>();
            new_runstate = *runstate;
        }

        match new_runstate {
            RunState::PreRun => {
                self.run_systems();
                self.ecs.maintain();
                new_runstate = RunState::AwaitingInput;
            }
            RunState::AwaitingInput => {
                new_runstate = player_input(self, ctx);
            }
            RunState::PlayerTurn => {
                self.run_systems();
                self.ecs.maintain();
                new_runstate = RunState::MonsterTurn;
            }
            RunState::MonsterTurn => {
                self.run_systems();
                self.ecs.maintain();
                new_runstate = RunState::AwaitingInput;
            }
            RunState::ShowInventory => {
                let result = ui::show_inventory(self, ctx);
                match result {
                    (ui::ItemMenuResult::Cancel, _) => new_runstate = RunState::AwaitingInput,
                    (ui::ItemMenuResult::NoResponse, _) | (ui::ItemMenuResult::Selected, None) => {}
                    (ui::ItemMenuResult::Selected, Some(entity)) => {
                        let mut intent = self.ecs.write_storage::<WantsToDrinkPotion>();
                        intent
                            .insert(
                                *self.ecs.fetch::<Entity>(),
                                WantsToDrinkPotion { potion: entity },
                            )
                            .expect("Unable to insert intent");
                        new_runstate = RunState::PlayerTurn;
                    }
                }
            }
            RunState::Dead => {}
        }

        {
            let mut runwriter = self.ecs.write_resource::<RunState>();
            *runwriter = new_runstate;
        }

        damage_system::delete_dead(&mut self.ecs);
    }
}

fn main() -> BError {
    let context = BTermBuilder::simple80x50().with_title("Explore").build()?;
    let mut gs = State { ecs: World::new() };
    gs.ecs.register::<BlocksTile>();
    gs.ecs.register::<CombatStats>();
    gs.ecs.register::<HealEffect>();
    gs.ecs.register::<InBackpack>();
    gs.ecs.register::<Item>();
    gs.ecs.register::<Monster>();
    gs.ecs.register::<Name>();
    gs.ecs.register::<Player>();
    gs.ecs.register::<Position>();
    gs.ecs.register::<Renderable>();
    gs.ecs.register::<SufferDamage>();
    gs.ecs.register::<Viewshed>();
    gs.ecs.register::<WantsToDisplayContent>();
    gs.ecs.register::<WantsToDrinkPotion>();
    gs.ecs.register::<WantsToMelee>();
    gs.ecs.register::<WantsToPickupItem>();

    gs.ecs.insert(RandomNumberGenerator::new());

    let map = Map::new_map(MAP_X, MAP_Y);
    let (player_x, player_y) = map.rooms[0].center();
    let player_entity = generator::spawn_player(&mut gs.ecs, player_x, player_y);

    gs.ecs.insert(player_entity);

    for room in map.rooms.iter().skip(1) {
        generator::spawn_room_contents(&mut gs.ecs, room);
    }
    gs.ecs.insert(map);
    gs.ecs.insert(Point::new(player_x, player_y));
    gs.ecs.insert(RunState::PreRun);
    gs.ecs.insert(gamelog::GameLog {
        entries: vec!["Welcome, traveller.".to_string()],
    });

    main_loop(context, gs)
}
