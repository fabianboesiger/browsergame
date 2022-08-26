use noise::{NoiseFn, OpenSimplex};
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, RngCore, SeedableRng};
use serde::{Deserialize, Serialize};

#[cfg(not(debug_assertions))]
const TICKS_PER_MINUTE: u32 = 60;
#[cfg(debug_assertions)]
const TICKS_PER_MINUTE: u32 = 1;

pub type UserId = i64;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventData {
    pub event: Event,
    pub user_id: Option<UserId>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Req {
    Event(Event),
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Res {
    Sync(SyncData),
    Event(EventData),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SyncData {
    pub user_id: UserId,
    pub state: State,
}

// MODIFY EVENTS AND STATE BELOW

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Map {
    pub tiles: Vec<Vec<Tile>>,
    pub n: i32,
}

impl Map {
    pub fn get_tile(&self, x: i32, y: i32) -> Option<&Tile> {
        if x < 0 || x >= self.n || y < 0 || y >= self.n {
            None
        } else {
            Some(&self.tiles[y as usize][x as usize])
        }
    }

    pub fn get_tile_mut(&mut self, x: i32, y: i32) -> Option<&mut Tile> {
        if x < 0 || x >= self.n || y < 0 || y >= self.n {
            None
        } else {
            Some(&mut self.tiles[y as usize][x as usize])
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Tile {
    pub tile_type: TileType,
    pub entities: HashSet<EntityId>,
}

impl Default for Map {
    fn default() -> Self {
        let n = 35;
        let mut rows = Vec::new();

        let height = OpenSimplex::new();

        for y in 0..n {
            let mut row = Vec::new();
            for x in 0..n {
                let height = (1.0
                    - ((x as f64 / n as f64 * 2.0 - 1.0).powi(2)
                        + (y as f64 / n as f64 * 2.0 - 1.0).powi(2)))
                    - (height
                        .get([x as f64 / 6.0, y as f64 / 6.0])
                        .min(0.5)
                        .max(-0.5)
                        + 0.5)
                        / 2.0;

                let tile_type = if height > 0.75 {
                    TileType::Mountain
                } else if height > 0.45 {
                    TileType::Forest
                } else if height > 0.2 {
                    TileType::Grassland
                } else if height > 0.0 {
                    TileType::Beach
                } else {
                    TileType::Water
                };

                row.push(Tile {
                    tile_type,
                    entities: HashSet::new(),
                })
            }
            rows.push(row);
        }

        Map { tiles: rows, n }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TileType {
    Water,
    Beach,
    Grassland,
    Forest,
    Mountain,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct State {
    pub map: Map,
    pub entities: HashMap<EntityId, Entity>,
    pub players: HashMap<UserId, Player>,
}

impl State {
    pub fn add_entity(&mut self, entity: Entity, rng: &mut SmallRng) {
        let entity_id = loop {
            let entity_id = rng.gen();
            if !self.entities.contains_key(&entity_id) {
                break entity_id;
            }
        };

        self.add_entity_with_id(entity, entity_id);
    }

    fn add_entity_with_id(&mut self, entity: Entity, entity_id: EntityId) {
        self.map
            .get_tile_mut(entity.x, entity.y)
            .unwrap()
            .entities
            .insert(entity_id);
        self.entities.insert(entity_id, entity);
    }

    pub fn remove_entity(&mut self, entity_id: &EntityId) -> Option<Entity> {
        if let Some(entity) = self.entities.remove(entity_id) {
            self.map
                .get_tile_mut(entity.x, entity.y)
                .unwrap()
                .entities
                .remove(entity_id);
            Some(entity)
        } else {
            None
        }
    }

    /*
    pub fn move_entity(&mut self, entity_id: &EntityId, direction: Direction) {
        if let Some(mut entity) = self.remove_entity(entity_id) {
            let (dx, dy) = direction.delta();
            entity.x =
                (entity.x + dx).max(0).min(self.map.n as i32 - 1);
            entity.y =
                (entity.y + dy).max(0).min(self.map.n as i32 - 1);

            self.add_entity_with_id(entity, *entity_id);
        }
    }
    */
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Player {
    pub username: String,
    pub money: u32,
    pub karma: i32,
    pub inventory: HashMap<ItemType, u32>,
}

impl Player {
    pub fn add_to_inventory<F: Fn(&ItemType) -> f64>(
        &mut self,
        rng: &mut SmallRng,
        range: std::ops::RangeInclusive<usize>,
        select: F,
    ) {
        let qty = rng.gen_range(range);
        for _ in 0..qty {
            let selected = ItemType::all().choose_weighted(rng, &select).unwrap();
            *self.inventory.entry(*selected).or_default() += 1;
        }
    }

    pub fn is_available_for_crafting(&self, item_type: ItemType, qty: u32) -> bool {
        if let Some(items) = item_type.crafting_requirements() {
            items
                .iter()
                .all(|(item, available_qty)| self.inventory.get(item).cloned().unwrap_or_default() * qty >= *available_qty)
        } else {
            false
        }
    }

    pub fn craft(&mut self, item_type: ItemType, qty: u32) -> bool {
        let all_available = self.is_available_for_crafting(item_type, qty);

        if all_available {
            for (item, qty2) in item_type.crafting_requirements().unwrap() {
                *self.inventory.entry(*item).or_default() -= qty * qty2;
            }

            *self.inventory.entry(item_type).or_default() += qty;
        }

        all_available
    }
}

pub type EntityId = u64;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entity {
    pub x: i32,
    pub y: i32,
    pub entity_type: EntityType,
}

impl Entity {
    fn get_as_person(&self) -> &Person {
        if let EntityType::Person(person) = &self.entity_type {
            person
        } else {
            panic!()
        }
    }

    fn get_as_person_mut(&mut self) -> &mut Person {
        if let EntityType::Person(person) = &mut self.entity_type {
            person
        } else {
            panic!()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum EntityType {
    Person(Person),
    Building(Building),
    Npc(Npc),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Person {
    pub first_name: String,
    pub last_name: String,
    pub health: i32,
    pub rest: i32,
    pub hunger: i32,
    pub tasks: VecDeque<Task>,
    //pub inventory: HashMap<ItemType, u32>,
    pub owner: UserId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Building {
    pub owner: UserId,
    pub remaining_time: Option<u32>,
    pub building_type: BuildingType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Npc {
    pub occupied_by: Option<UserId>,
    pub npc_type: NpcType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum NpcType {
    Boar,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum BuildingType {
    Castle,
}

impl Person {
    /*
    pub fn add_to_inventory<F: Fn(&ItemType) -> f64>(
        &mut self,
        rng: &mut SmallRng,
        range: std::ops::RangeInclusive<usize>,
        select: F,
    ) {
        let qty = rng.gen_range(range);
        for _ in 0..qty {
            let selected = ItemType::all().choose_weighted(rng, &select).unwrap();
            *self.inventory.entry(*selected).or_default() += 1;
        }
    }

    pub fn remove_from_inventory<F: Fn(&ItemType) -> f64>(
        &mut self,
        items: &[(ItemType, u32)],
    ) -> bool {
        let all_available = items
            .iter()
            .all(|(item, qty)| self.inventory.get(item).cloned().unwrap_or_default() >= *qty);

        if all_available {
            for (item, qty) in items {
                *self.inventory.entry(*item).or_default() -= qty;
            }
        }

        all_available
    }
    */
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ItemType {
    Blueberry,
    Mushroom,
    Wood,
    Fish,
    Crab,
    Shell,
    Apple,
    Stone,
    Coal,
    Iron,
    Gold,
    Crystal,
    Flower,
    Dagger,
}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ItemType::Blueberry => "Blueberry",
                ItemType::Mushroom => "Mushroom",
                ItemType::Wood => "Wood",
                ItemType::Fish => "Fish",
                ItemType::Crab => "Crab",
                ItemType::Shell => "Shell",
                ItemType::Apple => "Apple",
                ItemType::Stone => "Stone",
                ItemType::Coal => "Coal",
                ItemType::Iron => "Iron",
                ItemType::Gold => "Gold",
                ItemType::Flower => "Flower",
                ItemType::Crystal => "Crystal",
                ItemType::Dagger => "Dagger",
            }
        )
    }
}

impl ItemType {
    pub fn all() -> &'static [ItemType] {
        &[
            ItemType::Blueberry,
            ItemType::Mushroom,
            ItemType::Wood,
            ItemType::Fish,
            ItemType::Crab,
            ItemType::Shell,
            ItemType::Apple,
            ItemType::Stone,
            ItemType::Coal,
            ItemType::Iron,
            ItemType::Gold,
            ItemType::Crystal,
            ItemType::Flower,
            ItemType::Dagger,
        ]
    }

    pub fn crafting_requirements(&self) -> Option<&[(ItemType, u32)]> {
        match self {
            ItemType::Dagger => Some(&[(ItemType::Iron, 10)]),
            _ => None,
        }
    }

    pub fn offense(&self) -> u32 {
        match self {
            ItemType::Dagger => 5,
            _ => 0,
        }
    }

    pub fn defense(&self) -> u32 {
        match self {
            _ => 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ItemCategory {
    UpperBody,
    LowerBody,
    Feet,
    Head,
    Weapon,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Task {
    pub remaining_time: u32,
    pub task_type: TaskType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum TaskType {
    Walking(Direction),
    Gathering,
    Woodcutting,
    Fishing,
    Mining,
    Building(BuildingType),
    FightPerson(EntityId),
}

impl TaskType {
    pub fn duration(&self) -> u32 {
        match self {
            TaskType::Walking(_) => 10 * TICKS_PER_MINUTE,
            TaskType::Gathering => 10 * TICKS_PER_MINUTE,
            TaskType::Woodcutting => 10 * TICKS_PER_MINUTE,
            TaskType::Fishing => 10 * TICKS_PER_MINUTE,
            TaskType::Mining => 10 * TICKS_PER_MINUTE,
            TaskType::Building(_) => 10 * TICKS_PER_MINUTE,
            TaskType::FightPerson(_) => 10 * TICKS_PER_MINUTE,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Direction {
    North,
    South,
    East,
    West,
}

impl Direction {
    pub fn delta(&self) -> (i32, i32) {
        match self {
            Direction::North => (0, -1),
            Direction::South => (0, 1),
            Direction::East => (1, 0),
            Direction::West => (-1, 0),
        }
    }
}

/*
macro_rules! check_tasks {
    (self:expr, entity_id:expr, entity:expr, person:expr) => {
        loop {
            if let Some(task) = person.tasks.front() {
                println!("checking next task: {:?}", task.task_type);
                let ok = match &task.task_type {
                    TaskType::Walking(direction) => {
                        let (dx, dy) = direction.delta();
                        if let Some(next_tile) = self.map.get_tile(entity.x + dx, entity.y + dy)
                        {
                            match next_tile.tile_type {
                                TileType::Water => false,
                                _ => true,
                            }
                        } else {
                            false
                        }
                    }
                    TaskType::Gathering => {
                        if let Some(tile) = self.map.get_tile(entity.x, entity.y) {
                            match tile.tile_type {
                                TileType::Water => false,
                                _ => true,
                            }
                        } else {
                            false
                        }
                    }
                    TaskType::Woodcutting => {
                        if let Some(tile) = self.map.get_tile(entity.x, entity.y) {
                            match tile.tile_type {
                                TileType::Forest => true,
                                _ => false,
                            }
                        } else {
                            false
                        }
                    }
                    TaskType::Fishing => vec![
                        self.map.get_tile(entity.x + 1, entity.y),
                        self.map.get_tile(entity.x - 1, entity.y),
                        self.map.get_tile(entity.x, entity.y + 1),
                        self.map.get_tile(entity.x, entity.y - 1),
                    ]
                    .iter()
                    .filter_map(|x| *x)
                    .any(|t| t.tile_type == TileType::Water),
                    TaskType::Mining => {
                        if let Some(tile) = self.map.get_tile(entity.x, entity.y) {
                            match tile.tile_type {
                                TileType::Mountain => true,
                                _ => false,
                            }
                        } else {
                            false
                        }
                    }
                    TaskType::Building(BuildingType::Castle) => {
                        if let Some(tile) = self.map.get_tile(entity.x, entity.y) {
                            match tile.tile_type {
                                TileType::Water => false,
                                _ => true,
                            }
                        } else {
                            false
                        }
                    },
                    TaskType::Fighting(_) => {
                        true
                    }
                };

                println!("task was ok: {:?}", ok);

                if ok {
                    break;
                } else {
                    person.tasks.pop_front();
                }
            } else {
                break;
            }
        }
    };
}
*/

impl State {
    pub fn check_task(&self, entity_id: &EntityId, task_type: &TaskType) -> bool {
        if let Some(entity) = self.entities.get(entity_id) {
            if let EntityType::Person(person) = &entity.entity_type {
                println!("checking next task: {:?}", task_type);

                if let Some(next_task) = person.tasks.front() {
                    if let TaskType::Walking(_) = next_task.task_type {
                        if let TaskType::Walking(_) = task_type {
                            // Ok
                        } else {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }

                return match &task_type {
                    TaskType::Walking(direction) => {
                        let mut x = entity.x;
                        let mut y = entity.y;

                        for next_task in &person.tasks {
                            if let TaskType::Walking(direction) = &next_task.task_type {
                                let (dx, dy) = direction.delta();
                                x += dx;
                                y += dy;
                            }
                        }

                        let (dx, dy) = direction.delta();
                        if let Some(next_tile) = self.map.get_tile(x + dx, y + dy) {
                            match next_tile.tile_type {
                                TileType::Water => false,
                                _ => true,
                            }
                        } else {
                            false
                        }
                    }
                    TaskType::Gathering => {
                        if let Some(tile) = self.map.get_tile(entity.x, entity.y) {
                            match tile.tile_type {
                                TileType::Water => false,
                                _ => true,
                            }
                        } else {
                            false
                        }
                    }
                    TaskType::Woodcutting => {
                        if let Some(tile) = self.map.get_tile(entity.x, entity.y) {
                            match tile.tile_type {
                                TileType::Forest => true,
                                _ => false,
                            }
                        } else {
                            false
                        }
                    }
                    TaskType::Fishing => vec![
                        self.map.get_tile(entity.x + 1, entity.y),
                        self.map.get_tile(entity.x - 1, entity.y),
                        self.map.get_tile(entity.x, entity.y + 1),
                        self.map.get_tile(entity.x, entity.y - 1),
                    ]
                    .iter()
                    .filter_map(|x| *x)
                    .any(|t| t.tile_type == TileType::Water),
                    TaskType::Mining => {
                        if let Some(tile) = self.map.get_tile(entity.x, entity.y) {
                            match tile.tile_type {
                                TileType::Mountain => true,
                                _ => false,
                            }
                        } else {
                            false
                        }
                    }
                    TaskType::Building(BuildingType::Castle) => {
                        if let Some(tile) = self.map.get_tile(entity.x, entity.y) {
                            match tile.tile_type {
                                TileType::Water => false,
                                _ => true,
                            }
                        } else {
                            false
                        }
                    }
                    TaskType::FightPerson(_) => true,
                };
            }
        }

        false
    }

    /*
    pub fn remove_tasks(&mut self, entity_id: &EntityId) {
        loop {
            let entity = self.entities.get(entity_id).unwrap();
            let person = entity.get_as_person();

            if let Some(task) = person.tasks.front() {
                if self.check_task(entity_id, &task.task_type) {
                    break;
                } else {
                    self.entities.get_mut(entity_id).unwrap().get_as_person_mut().tasks.pop_front();
                }
            } else {
                break;
            }
        }
    }
    */

    pub fn update(&mut self, EventData { event, user_id }: EventData) {
        /*
        let check_task = |entity_id: &EntityId| {
            if let Some(entity) = self.entities.get_mut(entity_id) {
                if let EntityType::Person(person) = &mut entity.entity_type {

                }
            }
        };
        */

        match event {
            Event::AddPlayer(user_id, username) => {
                self.players.insert(
                    user_id,
                    Player {
                        username,
                        money: 0,
                        karma: 0,
                        inventory: HashMap::new(),
                    },
                );
            }
            Event::EditPlayer(user_id, username) => {
                self.players.get_mut(&user_id).unwrap().username = username;
            }
            Event::RemovePlayer(user_id) => {
                self.players.remove(&user_id);
            }
            Event::Tick(seed) => {
                let mut rng = SmallRng::seed_from_u64(seed);

                let mut entities_to_add = Vec::<Entity>::new();
                let mut entities_to_remove = Vec::<EntityId>::new();
                let entity_ids: Vec<EntityId> = self.entities.keys().cloned().collect();
                for entity_id in entity_ids {
                    match &self.entities.get(&entity_id).unwrap().entity_type {
                        EntityType::Person(person) => {
                            let owner = person.owner;

                            let person = self
                                .entities
                                .get_mut(&entity_id)
                                .unwrap()
                                .get_as_person_mut();

                            person.rest = (person.rest - 11).max(0).min(1_000_000);
                            person.hunger = (person.hunger - 11).max(0).min(1_000_000);

                            if person.rest > 0 {
                                if person.health < 1_000_000 {
                                    person.health = (person.health + 11).max(0).min(1_000_000);
                                }
                            }

                            let task_done = if let Some(Task { remaining_time, .. }) = self
                                .entities
                                .get_mut(&entity_id)
                                .unwrap()
                                .get_as_person_mut()
                                .tasks
                                .front_mut()
                            {
                                if *remaining_time == 0 {
                                    true
                                } else {
                                    *remaining_time -= 1;
                                    false
                                }
                            } else {
                                false
                            };

                            if task_done {
                                let Task { task_type, .. } = self
                                    .entities
                                    .get_mut(&entity_id)
                                    .unwrap()
                                    .get_as_person_mut()
                                    .tasks
                                    .pop_front()
                                    .unwrap();
                                match task_type {
                                    TaskType::Walking(direction) => {
                                        let entity = self.entities.get_mut(&entity_id).unwrap();

                                        let (dx, dy) = direction.delta();
                                        self.map
                                            .get_tile_mut(entity.x, entity.y)
                                            .unwrap()
                                            .entities
                                            .remove(&entity_id);
                                        entity.x =
                                            (entity.x + dx).max(0).min(self.map.n as i32 - 1);
                                        entity.y =
                                            (entity.y + dy).max(0).min(self.map.n as i32 - 1);
                                        self.map
                                            .get_tile_mut(entity.x, entity.y)
                                            .unwrap()
                                            .entities
                                            .insert(entity_id);
                                    }
                                    TaskType::Gathering => {
                                        println!("{:?} {:?}", entity_id, self.entities);
                                        let entity = self.entities.get(&entity_id).unwrap();

                                        self.players.get_mut(&owner).unwrap().add_to_inventory(
                                            &mut rng,
                                            1..=3,
                                            |item_type| match self
                                                .map
                                                .get_tile(entity.x, entity.y)
                                                .unwrap()
                                                .tile_type
                                            {
                                                TileType::Forest => match item_type {
                                                    ItemType::Blueberry => 20.0,
                                                    ItemType::Mushroom => 5.0,
                                                    _ => 0.0,
                                                },
                                                TileType::Beach => match item_type {
                                                    ItemType::Shell => 20.0,
                                                    _ => 0.0,
                                                },
                                                TileType::Grassland => match item_type {
                                                    ItemType::Flower => 20.0,
                                                    _ => 0.0,
                                                },
                                                TileType::Mountain => match item_type {
                                                    ItemType::Crystal => 1.0,
                                                    ItemType::Stone => 20.0,
                                                    _ => 0.0,
                                                },
                                                _ => 0.0,
                                            },
                                        );
                                    }
                                    TaskType::Woodcutting => {
                                        self.players.get_mut(&owner).unwrap().add_to_inventory(
                                            &mut rng,
                                            1..=3,
                                            |item_type| match item_type {
                                                ItemType::Wood => 20.0,
                                                ItemType::Apple => 5.0,
                                                _ => 0.0,
                                            },
                                        );
                                    }
                                    TaskType::Mining => {
                                        self.players.get_mut(&owner).unwrap().add_to_inventory(
                                            &mut rng,
                                            1..=3,
                                            |item_type| match item_type {
                                                ItemType::Coal => 20.0,
                                                ItemType::Iron => 5.0,
                                                ItemType::Gold => 5.0,
                                                _ => 0.0,
                                            },
                                        );
                                    }
                                    TaskType::Fishing => {
                                        self.players.get_mut(&owner).unwrap().add_to_inventory(
                                            &mut rng,
                                            1..=3,
                                            |item_type| match item_type {
                                                ItemType::Fish => 5.0,
                                                ItemType::Crab => 1.0,
                                                _ => 0.0,
                                            },
                                        );
                                    }
                                    TaskType::Building(building_type) => {
                                        let entity = self.entities.get(&entity_id).unwrap();

                                        entities_to_add.push(Entity {
                                            x: entity.x,
                                            y: entity.y,
                                            entity_type: EntityType::Building(Building {
                                                owner,
                                                remaining_time: None,
                                                building_type,
                                            }),
                                        });
                                    }
                                    TaskType::FightPerson(opponent_id) => {
                                        if entity_id < opponent_id {
                                            let entity = self.entities.get(&entity_id).unwrap();
                                            let opponent = self.entities.get(&opponent_id).unwrap();
                                        }
                                    }
                                }

                                //self.remove_tasks(&entity_id);
                            }
                        }
                        EntityType::Building(building) => {
                            /*
                            let remove_building =
                                if let Some(remaining_time) = &mut building.remaining_time {
                                    if *remaining_time == 0 {
                                        true
                                    } else {
                                        *remaining_time -= 1;
                                        false
                                    }
                                } else {
                                    false
                                };

                            if remove_building {
                                entities_to_remove.push(*entity_id);
                            }
                            */
                        }
                        _ => {}
                    }
                }

                for entity_id in entities_to_remove {
                    self.remove_entity(&entity_id);
                }
                for entity in entities_to_add {
                    self.add_entity(entity, &mut rng);
                }
            }
            Event::RandRes(seed, event) => {
                let mut rng = SmallRng::seed_from_u64(seed);

                match event {
                    RandEvent::SpawnPerson => {
                        let entity = Entity {
                            x: (rng.next_u32() % self.map.n as u32) as i32,
                            y: (rng.next_u32() % self.map.n as u32) as i32,

                            entity_type: EntityType::Person(Person {
                                owner: user_id.unwrap(),
                                first_name: FIRST_NAMES.choose(&mut rng).unwrap().to_string(),
                                last_name: LAST_NAMES.choose(&mut rng).unwrap().to_string(),
                                ..Person::default()
                            }),
                        };
                        self.add_entity(entity, &mut rng);
                    }
                }
            }
            Event::RandReq(_) => unreachable!(),
            Event::PushTask(entity_id, task_type) => {
                let ok = self.check_task(&entity_id, &task_type);
                if let Some(entity) = self.entities.get_mut(&entity_id) {
                    if let EntityType::Person(person) = &mut entity.entity_type {
                        if ok {
                            person.tasks.push_back(Task {
                                remaining_time: task_type.duration(),
                                task_type,
                            });
                        }
                    }
                }
            }
            Event::PopTask(entity_id) => {
                if let Some(entity) = self.entities.get_mut(&entity_id) {
                    if let EntityType::Person(person) = &mut entity.entity_type {
                        person.tasks.pop_back();
                    }
                }
            }
            Event::ChallengeToFight(challenger_entity_id, challenged_entity_id) => {
                let accept_challenge =
                    vec![challenged_entity_id, challenged_entity_id]
                        .iter()
                        .all(|entity_id| {
                            if let Some(entity) = self.entities.get(&entity_id) {
                                match &entity.entity_type {
                                    EntityType::Person(person) => {
                                        if let Some(task) = person.tasks.front() {
                                            match task.task_type {
                                                TaskType::FightPerson(_) => false,
                                                _ => true,
                                            }
                                        } else {
                                            true
                                        }
                                    }
                                    _ => false,
                                }
                            } else {
                                false
                            }
                        });

                if accept_challenge {
                    if let Some(entity) = self.entities.get_mut(&challenger_entity_id) {
                        if let EntityType::Person(person) = &mut entity.entity_type {
                            let task_type = TaskType::FightPerson(challenged_entity_id);
                            person.tasks.push_front(Task {
                                remaining_time: task_type.duration(),
                                task_type,
                            });
                        }
                    }
                    if let Some(entity) = self.entities.get_mut(&challenged_entity_id) {
                        if let EntityType::Person(person) = &mut entity.entity_type {
                            let task_type = TaskType::FightPerson(challenger_entity_id);
                            person.tasks.push_front(Task {
                                remaining_time: task_type.duration(),
                                task_type,
                            });
                        }
                    }
                }
            }
            Event::CraftItem(item_type, qty) => {
                if let Some(requirements) = item_type.crafting_requirements() {
                    let player = &mut self.players.get_mut(&user_id.unwrap()).unwrap();

                    player.craft(item_type, qty);   
                }
            }
        }
    }

    pub fn view(&self, _receiver: UserId) -> Self {
        State { ..self.clone() }
    }
}

pub type Seed = u64;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Event {
    Tick(Seed),
    RandRes(Seed, RandEvent),
    RandReq(RandEvent),
    AddPlayer(UserId, String),
    EditPlayer(UserId, String),
    RemovePlayer(UserId),
    PushTask(EntityId, TaskType),
    PopTask(EntityId),
    ChallengeToFight(EntityId, EntityId),
    CraftItem(ItemType, u32),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RandEvent {
    SpawnPerson,
}

impl EventData {
    pub fn filter(&self, _receiver: UserId) -> bool {
        /*
        let EventData { event, user_id } = self;
        let user_id = *user_id;

        match event {
            _ => true,
        }
        */
        true
    }
}

const FIRST_NAMES: &'static [&'static str] = &[
    "Ruben",
    "Andros",
    "Nate",
    "Aldwin",
    "Ben",
    "Bastian",
    "Bronn",
    "Draco",
    "Edward",
    "Falkor",
    "Finn",
    "Gandalf",
    "Gregor",
    "Tormund",
    "Arya",
    "Brienne",
    "Catelyn",
    "Gilly",
    "Margaery",
    "Olenna",
    "Elisabeth",
    "Henry",
    "Cateline",
    "Estienne",
];

const LAST_NAMES: &'static [&'static str] = &[
    "Dupois",
    "Booker",
    "Endo",
    "Gannon",
    "Bauer",
    "Brown",
    "Chandler",
    "Everett",
    "Fox",
    "Fisher",
    "Kemp",
    "Knight",
    "Lancaster",
    "Perker",
    "Ryder",
    "Smith",
    "Steele",
    "Sommer",
    "Brewer",
    "Hill",
    "Klein",
    "De Metz",
    "Odson",
];
