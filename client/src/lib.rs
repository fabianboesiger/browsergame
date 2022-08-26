use seed::{prelude::*, *};
use shared::{
    BuildingType, Direction, Entity, EntityType, Event, EventData, ItemType, NpcType, Person,
    RandEvent, Req, Res, SyncData, Task, TaskType, TileType,
};
use std::{collections::HashMap, rc::Rc};

const WS_URL: &str = "ws://127.0.0.1:3000/game/ws";

// ------ ------
//     Model
// ------ ------

#[derive(Debug, PartialEq, Eq)]
pub enum Page {
    Map(Option<(i32, i32)>),
    Exilants,
    Inventory,
}

#[derive(Debug, Clone, Copy)]
pub struct Map {
    center: (i32, i32),
    radius: i32,
    n: i32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Inventory {
    crafting_item_type: Option<ItemType>,
    crafting_quantity: u32,
}

impl Map {
    fn new(n: i32) -> Self {
        Map {
            center: (n / 2, n / 2),
            radius: n / 2,
            n,
        }
    }

    fn normalize(&mut self) {
        self.radius = self.radius.max(2).min(self.n / 2);
        self.center.0 = self.center.0.max(self.radius).min(self.n - 1 - self.radius);
        self.center.1 = self.center.1.max(self.radius).min(self.n - 1 - self.radius);
    }
}

pub struct Model {
    web_socket: WebSocket,
    web_socket_reconnector: Option<StreamHandle>,
    state: Option<SyncData>,
    page: Page,
    map: Map,
    inventory: Inventory,
}

// ------ ------
//     Init
// ------ ------

fn init(_url: Url, orders: &mut impl Orders<Msg>) -> Model {
    orders.subscribe(|subs::UrlRequested(_, url_request)| url_request.handled());

    Model {
        web_socket: create_websocket(orders),
        web_socket_reconnector: None,
        state: None,
        page: Page::Map(None),
        map: Map::new(1),
        inventory: Inventory::default()
    }
}

// ------ ------
//    Update
// ------ ------

pub enum Msg {
    WebSocketOpened,
    CloseWebSocket,
    WebSocketClosed(CloseEvent),
    WebSocketFailed,
    ReconnectWebSocket(usize),
    SendGameEvent(Event),
    ReceiveGameEvent(EventData),
    InitGameState(SyncData),
    ChangePage(Page),
    ChangeMap(Map),
    ChangeInventory(Inventory),
}

fn update(msg: Msg, mut model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::WebSocketOpened => {
            model.web_socket_reconnector = None;
            log!("WebSocket connection is open now");
        }
        Msg::CloseWebSocket => {
            model.web_socket_reconnector = None;
            model
                .web_socket
                .close(None, Some("user clicked close button"))
                .unwrap();
        }
        Msg::WebSocketClosed(close_event) => {
            log!(
                "WebSocket connection was closed, reason:",
                close_event.reason()
            );

            // Chrome doesn't invoke `on_error` when the connection is lost.
            if !close_event.was_clean() && model.web_socket_reconnector.is_none() {
                model.web_socket_reconnector = Some(
                    orders.stream_with_handle(streams::backoff(None, Msg::ReconnectWebSocket)),
                );
            }
        }
        Msg::WebSocketFailed => {
            log!("WebSocket failed");
            if model.web_socket_reconnector.is_none() {
                model.web_socket_reconnector = Some(
                    orders.stream_with_handle(streams::backoff(None, Msg::ReconnectWebSocket)),
                );
            }
        }
        Msg::ReconnectWebSocket(retries) => {
            log!("Reconnect attempt:", retries);
            model.web_socket = create_websocket(orders);
        }
        Msg::SendGameEvent(event) => {
            let serialized = rmp_serde::to_vec(&Req::Event(event)).unwrap();
            model.web_socket.send_bytes(&serialized).unwrap();
        }
        Msg::ReceiveGameEvent(event) => {
            if let Some(SyncData { state, .. }) = &mut model.state {
                state.update(event);
            }
        }
        Msg::InitGameState(sync_data) => {
            model.map = Map::new(sync_data.state.map.n);
            model.state = Some(sync_data);
        }
        Msg::ChangePage(page) => {
            if let Page::Map(Some(center)) = &page {
                model.map.center = *center;
                model.map.radius = 2;
                model.map.normalize();
            };
            model.page = page;
        }
        Msg::ChangeMap(mut map) => {
            map.normalize();
            model.map = map;
        }
        Msg::ChangeInventory(inventory) => {
            model.inventory = inventory;
        }
    }
}

fn create_websocket(orders: &impl Orders<Msg>) -> WebSocket {
    let msg_sender = orders.msg_sender();

    WebSocket::builder(WS_URL, orders)
        .on_open(|| Msg::WebSocketOpened)
        .on_message(move |msg| decode_message(msg, msg_sender))
        .on_close(Msg::WebSocketClosed)
        .on_error(|| Msg::WebSocketFailed)
        .build_and_open()
        .unwrap()
}

fn decode_message(message: WebSocketMessage, msg_sender: Rc<dyn Fn(Option<Msg>)>) {
    if message.contains_text() {
        unreachable!()
    } else {
        spawn_local(async move {
            let bytes = message
                .bytes()
                .await
                .expect("WebsocketError on binary data");

            let msg: Res = rmp_serde::from_slice(&bytes).unwrap();
            match msg {
                Res::Event(event) => {
                    msg_sender(Some(Msg::ReceiveGameEvent(event)));
                }
                Res::Sync(sync) => {
                    msg_sender(Some(Msg::InitGameState(sync)));
                }
            }
        });
    }
}

// ------ ------
//     View
// ------ ------

fn view(model: &Model) -> Vec<Node<Msg>> {
    if let Some(data) = &model.state {
        vec![
            nav(model, data),
            match model.page {
                Page::Map(_) => map_zoomable(data, &model.map),
                Page::Exilants => exilants(data),
                Page::Inventory => inventory(data, &model.inventory),
            },
        ]
    } else {
        vec![p!["Loading ..."]]
    }
}

fn exilants(data @ SyncData { user_id, state }: &SyncData) -> Node<Msg> {
    let persons = state.entities.iter().filter_map(|(entity_id, entity)| {
        if let EntityType::Person(person @ Person { owner, .. }) = &entity.entity_type {
            if owner == user_id {
                return Some((entity_id, entity, person));
            }
        }
        None
    });

    section![if persons.clone().count() == 0 {
        div![
            p!["You have no exilants at the moment!"],
            button![
                C!["button"],
                ev(Ev::Click, move |_| Msg::SendGameEvent(Event::RandReq(
                    RandEvent::SpawnPerson
                ))),
                "Exile a poor soul to get started"
            ]
        ]
    } else {
        div![persons.map(|(&entity_id, entity, person)| {
            let center = (entity.x, entity.y);

            div![
                C!["exilant"],
                h2![format!("{} {}", person.first_name, person.last_name)],
                h3!["Environment"],
                div![
                    C!["exilant-section"],
                    div![map(
                        data,
                        &{
                            let mut map = Map {
                                center,
                                radius: 2,
                                n: state.map.n,
                            };
                            map.normalize();
                            map
                        },
                        String::from("200px")
                    ),],
                    div![
                        p!["There is nothing to see here."],
                        button![
                            ev(Ev::Click, move |_| Msg::ChangePage(Page::Map(Some(center)))),
                            "View on map"
                        ],
                    ]
                ],
                h3!["Equipment"],
                div![],
                h3!["Tasks"],
                div![
                    C!["exilant-section"],
                    div![
                        fieldset![
                            legend!["Travel"],
                            button![
                                if !state
                                    .check_task(&entity_id, &TaskType::Walking(Direction::North))
                                {
                                    attrs! {
                                        At::Disabled => ""
                                    }
                                } else {
                                    attrs! {}
                                },
                                ev(Ev::Click, move |_| Msg::SendGameEvent(Event::PushTask(
                                    entity_id,
                                    TaskType::Walking(Direction::North)
                                ))),
                                "North"
                            ],
                            button![
                                if !state
                                    .check_task(&entity_id, &TaskType::Walking(Direction::South))
                                {
                                    attrs! {
                                        At::Disabled => ""
                                    }
                                } else {
                                    attrs! {}
                                },
                                ev(Ev::Click, move |_| Msg::SendGameEvent(Event::PushTask(
                                    entity_id,
                                    TaskType::Walking(Direction::South)
                                ))),
                                "South"
                            ],
                            button![
                                if !state
                                    .check_task(&entity_id, &TaskType::Walking(Direction::East))
                                {
                                    attrs! {
                                        At::Disabled => ""
                                    }
                                } else {
                                    attrs! {}
                                },
                                ev(Ev::Click, move |_| Msg::SendGameEvent(Event::PushTask(
                                    entity_id,
                                    TaskType::Walking(Direction::East)
                                ))),
                                "East"
                            ],
                            button![
                                if !state
                                    .check_task(&entity_id, &TaskType::Walking(Direction::West))
                                {
                                    attrs! {
                                        At::Disabled => ""
                                    }
                                } else {
                                    attrs! {}
                                },
                                ev(Ev::Click, move |_| Msg::SendGameEvent(Event::PushTask(
                                    entity_id,
                                    TaskType::Walking(Direction::West)
                                ))),
                                "West"
                            ],
                        ],
                        fieldset![
                            legend!["Laboring"],
                            button![
                                if !state.check_task(&entity_id, &TaskType::Gathering) {
                                    attrs! {
                                        At::Disabled => ""
                                    }
                                } else {
                                    attrs! {}
                                },
                                ev(Ev::Click, move |_| Msg::SendGameEvent(Event::PushTask(
                                    entity_id,
                                    TaskType::Gathering
                                ))),
                                "Gathering"
                            ],
                            button![
                                if !state.check_task(&entity_id, &TaskType::Fishing) {
                                    attrs! {
                                        At::Disabled => ""
                                    }
                                } else {
                                    attrs! {}
                                },
                                ev(Ev::Click, move |_| Msg::SendGameEvent(Event::PushTask(
                                    entity_id,
                                    TaskType::Fishing
                                ))),
                                "Fishing"
                            ],
                            button![
                                if !state.check_task(&entity_id, &TaskType::Woodcutting) {
                                    attrs! {
                                        At::Disabled => ""
                                    }
                                } else {
                                    attrs! {}
                                },
                                ev(Ev::Click, move |_| Msg::SendGameEvent(Event::PushTask(
                                    entity_id,
                                    TaskType::Woodcutting
                                ))),
                                "Woodcutting"
                            ],
                            button![
                                if !state.check_task(&entity_id, &TaskType::Mining) {
                                    attrs! {
                                        At::Disabled => ""
                                    }
                                } else {
                                    attrs! {}
                                },
                                ev(Ev::Click, move |_| Msg::SendGameEvent(Event::PushTask(
                                    entity_id,
                                    TaskType::Mining
                                ))),
                                "Mining"
                            ],
                        ]
                    ],
                    div![if person.tasks.len() == 0 {
                        p![format!(
                            "{} {} is not doing anything at the moment.",
                            person.first_name, person.last_name
                        )]
                    } else {
                        div![
                            ul![person.tasks.iter().map(
                                |Task {
                                     remaining_time,
                                     task_type,
                                 }| {
                                    li![format!(
                                        "{} ({}s)",
                                        match task_type {
                                            TaskType::Walking(Direction::North) =>
                                                "Travelling north".to_owned(),
                                            TaskType::Walking(Direction::South) =>
                                                "Travelling south".to_owned(),
                                            TaskType::Walking(Direction::East) =>
                                                "Travelling east".to_owned(),
                                            TaskType::Walking(Direction::West) =>
                                                "Travelling west".to_owned(),
                                            TaskType::Gathering => "Gathering".to_owned(),
                                            TaskType::Woodcutting => "Woodcutting".to_owned(),
                                            TaskType::Fishing => "Fishing".to_owned(),
                                            TaskType::Mining => "Mining".to_owned(),
                                            TaskType::Building(BuildingType::Castle) =>
                                                "Building a castle".to_owned(),
                                            TaskType::FightPerson(opponent) => {
                                                if let Some(opponent) = state.entities.get(opponent)
                                                {
                                                    match &opponent.entity_type {
                                                        EntityType::Person(person) => format!(
                                                            "Fighting {} {}",
                                                            person.first_name, person.last_name
                                                        ),
                                                        _ => unreachable!(),
                                                    }
                                                } else {
                                                    unreachable!()
                                                }
                                            }
                                        },
                                        remaining_time
                                    )]
                                }
                            )],
                            button![
                                ev(Ev::Click, move |_| Msg::SendGameEvent(Event::PopTask(
                                    entity_id
                                ))),
                                "Cancel last task"
                            ],
                        ]
                    }]
                ],
                /*
                h3!["Inventory"],
                ul![person
                    .inventory
                    .iter()
                    .map(|(item_type, qty)| { li![format!("{} ({})", item_type, qty)] })],
                */
            ]
        })]
    }]
}

fn nav(model: &Model, SyncData { state, user_id }: &SyncData) -> Node<Msg> {
    let player = state.players.get(user_id).unwrap();

    nav![
        div![
            button![
                if let Page::Map { .. } = model.page {
                    C!["selected"]
                } else {
                    C![]
                },
                ev(Ev::Click, move |_| Msg::ChangePage(Page::Map(None))),
                //"Map",
                icon("map-marker")
            ],
            button![
                if let Page::Exilants { .. } = model.page {
                    C!["selected"]
                } else {
                    C![]
                },
                ev(Ev::Click, move |_| Msg::ChangePage(Page::Exilants)),
                //"Exilants",
                icon("people")
            ],
            button![
                if let Page::Inventory { .. } = model.page {
                    C!["selected"]
                } else {
                    C![]
                },
                ev(Ev::Click, move |_| Msg::ChangePage(Page::Inventory)),
                //"Inventory",
                icon("rucksack")
            ],
            a![
                attrs! {At::Href => "/account"},
                //"Account",
                icon("settings")
            ]
        ],
        div![
            span![icon("coins"), " ", player.money],
            span![icon("yin-yang"), " ", player.karma],
        ]
    ]
}

fn map(
    SyncData { state, .. }: &SyncData,
    Map { center, radius, .. }: &Map,
    viewport: String,
) -> Node<Msg> {
    let r = *radius;
    let (x, y) = *center;
    let left = (x - r) as usize;
    let top = (y - r) as usize;
    let width = (r * 2 + 1) as usize;
    let height = (r * 2 + 1) as usize;

    let mut map = HashMap::<(i32, i32), Vec<&EntityType>>::new();
    for Entity { x, y, entity_type } in state.entities.values() {
        map.entry((*x, *y)).or_default().push(entity_type);
    }

    for entities in map.values_mut() {
        entities.sort_by_key(|entity_type| match entity_type {
            EntityType::Building(_) => 0,
            EntityType::Person(_) => 1,
            EntityType::Npc(_) => 2,
        });
    }

    log!(format!("{:?}", map));

    section![
        id!["map"],
        table![
            C!["map"],
            state
                .map
                .tiles
                .iter()
                .enumerate()
                .skip(top)
                .take(height)
                .map(
                    |(y, row)| tr![row.iter().enumerate().skip(left).take(width).map(
                        |(x, tile)| td![
                            style! {
                                St::Width => format!("calc({} / {})", viewport, width),
                                St::Height => format!("calc({} / {})", viewport, height),
                            },
                            C![
                                "tile",
                                match tile.tile_type {
                                    TileType::Mountain => "mountain",
                                    TileType::Water => "water",
                                    TileType::Beach => "beach",
                                    TileType::Grassland => "grassland",
                                    TileType::Forest => "forest",
                                }
                            ],
                            map.get(&(x as i32, y as i32))
                                .unwrap_or(&Vec::new())
                                .iter()
                                .map(|entity_type| match entity_type {
                                    EntityType::Person(_) => "P",
                                    EntityType::Building(building) =>
                                        match building.building_type {
                                            BuildingType::Castle => "C",
                                        },
                                    EntityType::Npc(npc) => match npc.npc_type {
                                        NpcType::Boar => "B",
                                    },
                                })
                        ]
                    )]
                )
        ]
    ]
}

fn map_zoomable(data: &SyncData, m @ Map { center, radius, .. }: &Map) -> Node<Msg> {
    let r = *radius;
    let (x, y) = *center;
    let m = *m;

    div![
        map(data, &m, String::from("min(100vw, 1024px)")),
        table![
            C!["controls"],
            tbody![
                tr![
                    td![],
                    td![
                        C!["button-wrapper"],
                        button![
                            ev(Ev::Click, move |_| Msg::ChangeMap(Map {
                                center: (x, y - 1),
                                ..m
                            })),
                            "N"
                        ]
                    ],
                    td![],
                    td![],
                    td![],
                    td![],
                ],
                tr![
                    td![
                        C!["button-wrapper"],
                        button![
                            ev(Ev::Click, move |_| Msg::ChangeMap(Map {
                                center: (x - 1, y),
                                ..m
                            })),
                            "W"
                        ]
                    ],
                    td![],
                    td![
                        C!["button-wrapper"],
                        button![
                            ev(Ev::Click, move |_| Msg::ChangeMap(Map {
                                center: (x + 1, y),
                                ..m
                            })),
                            "E"
                        ]
                    ],
                    td![],
                    td![
                        C!["button-wrapper"],
                        button![
                            ev(Ev::Click, move |_| Msg::ChangeMap(Map {
                                radius: r - 1,
                                ..m
                            })),
                            "+"
                        ]
                    ],
                    td![
                        C!["button-wrapper"],
                        button![
                            ev(Ev::Click, move |_| Msg::ChangeMap(Map {
                                radius: r + 1,
                                ..m
                            })),
                            "-"
                        ]
                    ],
                ],
                tr![
                    td![],
                    td![
                        C!["button-wrapper"],
                        button![
                            ev(Ev::Click, move |_| Msg::ChangeMap(Map {
                                center: (x, y + 1),
                                ..m
                            })),
                            "S"
                        ]
                    ],
                    td![],
                    td![],
                    td![],
                    td![],
                ]
            ]
        ],
    ]
}

fn inventory(SyncData { state, user_id }: &SyncData, inventory: &Inventory) -> Node<Msg> {
    let crafting_item_type = inventory.crafting_item_type;
    let crafting_quantity = inventory.crafting_quantity;

    div![
        ul![state
            .players
            .get(&user_id)
            .unwrap()
            .inventory
            .iter()
            .map(|(item_type, qty)| { li![format!("{} ({})", item_type, qty)] })],
        div![
            C!["form-wrapper"],
            h2!["Crafting"],
            label![
                attrs! {
                    At::For => "type"
                },
                "Item"
            ],
            select![
                attrs! {
                    At::Id => "type"
                    At::Value => crafting_item_type.map(|item_type| format!("{}", item_type)).unwrap_or_default()
                },
                input_ev(Ev::Input, move |input| Msg::ChangeInventory(Inventory {
                    crafting_quantity: 1,
                    crafting_item_type: ItemType::all().iter().find(|item_type| format!("{}", item_type) == input).cloned()
                })),
                ItemType::all()
                    .iter()
                    .filter(|item_type| item_type.crafting_requirements().is_some())
                    .map(|item_type| { option![
                        attrs!{At::Value => format!("{}", item_type)},
                        format!("{}", item_type)
                    ] })
            ],
            label![
                attrs! {
                    At::For => "quantity"
                    At::Value => inventory.crafting_quantity
                },
                "Amount"
            ],
            input![
                attrs! {
                    At::Id => "quantity"
                    At::Type => "number"
                    At::Value => 0
                },
                input_ev(Ev::Input, move |input| Msg::ChangeInventory(Inventory {
                    crafting_quantity: input.parse().unwrap(),
                    crafting_item_type
                })),
            ],
            if let Some(crafting_item_type) = crafting_item_type {
                if state.players.get(user_id).unwrap().is_available_for_crafting(crafting_item_type, crafting_quantity) {
                    button![
                        ev(Ev::Click, move |_| Msg::SendGameEvent(Event::CraftItem(crafting_item_type, crafting_quantity))),
                        "Craft"
                    ]
                } else {
                    button![
                        attrs! {
                            At::Disabled => "disabled"
                        },
                        "Craft"
                    ]
                }
            } else {
                button![
                    attrs! {
                        At::Disabled => "disabled"
                    },
                    "Craft"
                ]
            }
            
        ]
    ]
}

fn icon(icon: &'static str) -> Node<Msg> {
    img![
        attrs! {
            At::Src => format!("/icons/icons8-{}-24.png", icon)
        },
        icon
    ]
}

// ------ ------
//     Start
// ------ ------

#[wasm_bindgen(start)]
pub fn start() {
    App::start("app", init, update, view);
}
