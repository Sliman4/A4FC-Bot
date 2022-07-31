#![allow(deprecated)] // serenity uses deprecated type aliases in model::prelude

use async_trait::async_trait;
use dashmap::DashMap;
use dotenv::dotenv;
use rand::prelude::SliceRandom;
use serde::Deserialize;
use serenity::builder::CreateButton;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::env;
use std::time::{Duration, Instant};

const COMMAND_SETUP_FAN_APPLICATION_CHANNEL: &str = "setupfanapplicationchannel";
const BUTTON_FAN_APPLICATION: &str = "buttonsetupapplication";
const FAN_APPLICATION_QUESTIONS: usize = 5;
const FAN_APPLICATION_MINUTES: u64 = 5;
const FAN_ROLE: RoleId = RoleId(945303640510435398);

struct FanClubBot {
    fan_questions: Vec<Question>,
    fan_applications: DashMap<UserId, FanApplication>,
}

#[derive(Deserialize)]
struct BotConfig {
    fan_questions: Vec<Question>,
}

#[derive(Clone, Debug, Deserialize)]
struct Question {
    question: String,
    answers: Vec<String>,
    correct_answer: usize,
}

struct FanApplication {
    questions: [Question; FAN_APPLICATION_QUESTIONS],
    current_question: usize,
    start_time: Instant,
}

#[async_trait]
impl EventHandler for FanClubBot {
    async fn ready(&self, ctx: Context, _data_about_bot: Ready) {
        let guild_id = GuildId(945274214913560657);
        let commands = guild_id
            .set_application_commands(&ctx.http, |commands| {
                commands.create_application_command(|command| {
                    command
                        .name(COMMAND_SETUP_FAN_APPLICATION_CHANNEL)
                        .description("Setup fan bot")
                        .default_member_permissions(Permissions::ADMINISTRATOR)
                })
            })
            .await
            .expect("Couldn't register a command");
        println!("{} commands registered", commands.len());
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::ApplicationCommand(command_interaction) => {
                match &command_interaction.data.name[..] {
                    COMMAND_SETUP_FAN_APPLICATION_CHANNEL => {
                        setup_fan_application_channel(&ctx, &command_interaction).await;
                    }
                    unknown => println!("Unknown command: {unknown}"),
                }
            }
            Interaction::MessageComponent(component_interaction) => {
                match &component_interaction.data.custom_id[..] {
                    BUTTON_FAN_APPLICATION => {
                        assert!(self.fan_questions.len() >= FAN_APPLICATION_QUESTIONS);
                        let mut questions_index = (0..self.fan_questions.len()).collect::<Vec<_>>();
                        questions_index.shuffle(&mut rand::thread_rng());
                        let questions = questions_index
                            .into_iter()
                            .take(FAN_APPLICATION_QUESTIONS)
                            .map(|i| &self.fan_questions[i])
                            .cloned()
                            .collect::<Vec<_>>();
                        let application_state = FanApplication {
                            questions: questions.try_into().unwrap(),
                            current_question: 0,
                            start_time: Instant::now(),
                        };
                        let user_id = component_interaction
                            .member
                            .as_ref()
                            .expect("Unable to get interaction author")
                            .user
                            .id;
                        self.fan_applications.insert(user_id, application_state);
                        let application_state = self.fan_applications.get(&user_id).unwrap();
                        ask_question(
                            &ctx,
                            &component_interaction,
                            &application_state.questions[application_state.current_question],
                        )
                        .await;
                    }
                    "answer0" | "answer1" | "answer2" | "answer3" => {
                        let i = component_interaction
                            .data
                            .custom_id
                            .chars()
                            .nth(6)
                            .unwrap()
                            .to_string()
                            .parse::<usize>()
                            .unwrap();

                        let mut member = component_interaction
                            .member
                            .clone()
                            .expect("Couldn't get interaction author");
                        let mut application_state =
                            self.fan_applications.get_mut(&member.user.id).unwrap();

                        let current_question =
                            &application_state.questions[application_state.current_question];
                        assert!(i < current_question.answers.len());

                        if current_question.correct_answer == i {
                            if application_state.current_question + 1
                                == application_state.questions.len()
                            {
                                if application_state.start_time
                                    + Duration::from_secs(60 * FAN_APPLICATION_MINUTES)
                                    > Instant::now()
                                {
                                    if !member.roles.contains(&FAN_ROLE) {
                                        member
                                            .add_role(&ctx.http, FAN_ROLE)
                                            .await
                                            .expect("Unable to add a role");
                                    }
                                    component_interaction
                                        .create_interaction_response(&ctx.http, |response| {
                                            response
                                            .kind(InteractionResponseType::ChannelMessageWithSource)
                                            .interaction_response_data(|message| {
                                                message
                                                    .flags(interaction::MessageFlags::EPHEMERAL)
                                                    .content("Роль выдана!")
                                            })
                                        })
                                        .await
                                        .expect("Couldn't send message")
                                } else {
                                    component_interaction.create_interaction_response(&ctx.http, |response| {
                                        response
                                            .kind(InteractionResponseType::ChannelMessageWithSource)
                                            .interaction_response_data(|message| {
                                                message
                                                    .flags(interaction::MessageFlags::EPHEMERAL)
                                                    .content("Все верно! Но 5 минут прошло. Попробуй еще раз через 7 дней!")
                                            })
                                    }).await.expect("Couldn't send message")
                                }
                                drop(application_state);
                                self.fan_applications.remove(&member.user.id).unwrap();
                            } else {
                                application_state.current_question += 1;
                                ask_question(
                                    &ctx,
                                    &component_interaction,
                                    &application_state.questions
                                        [application_state.current_question],
                                )
                                .await;
                            }
                        } else {
                            component_interaction.create_interaction_response(&ctx.http, |response| {
                                response
                                    .kind(InteractionResponseType::ChannelMessageWithSource)
                                    .interaction_response_data(|message| {
                                        message
                                            .flags(interaction::MessageFlags::EPHEMERAL)
                                            .content("Неправильный ответ! Посмотри больше видео и попробуй еще раз через 7 дней!")
                                    })
                            }).await.expect("Couldn't send message");
                        }
                    }
                    unknown => println!("Unknown component interaction: {unknown}"),
                }
            }
            _ => (),
        }
    }
}

async fn setup_fan_application_channel(
    ctx: &Context,
    interaction: &interaction::application_command::ApplicationCommandInteraction,
) {
    interaction
        .channel_id
        .send_message(&ctx.http, |message| {
            message
                .content(format!("\
                Если хочешь стать более уважаемым членом сообщества, открыть каналы для истинных фанатов и получить роль <@&{fan_role}>, нажми эту кнопку.\n\
                После нажатия кнопки тебе зададут {FAN_APPLICATION_QUESTIONS} простых вопросов по A4, на которые тебе нужно будет правильно ответить за {FAN_APPLICATION_MINUTES} минут.\
                ", fan_role = FAN_ROLE.0))
                .components(|components| {
                    components.create_action_row(|row| {
                        row.add_button({
                            let mut button = CreateButton::default();
                            button.custom_id(BUTTON_FAN_APPLICATION)
                                .emoji('✅')
                                .style(component::ButtonStyle::Success)
                                .label("Стать фанатом");
                            button
                        })
                    })
                })
        }).await.expect("Couldn't send message");
}

async fn ask_question(
    ctx: &Context,
    interaction: &interaction::message_component::MessageComponentInteraction,
    question: &Question,
) {
    interaction
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message
                        .flags(interaction::MessageFlags::EPHEMERAL)
                        .content(&question.question)
                        .components(|components| {
                            components.create_action_row(|row| {
                                let mut answers =
                                    question.answers.iter().enumerate().collect::<Vec<_>>();
                                if answers.len() == 4 {
                                    answers.shuffle(&mut rand::thread_rng());
                                }
                                for (i, answer) in answers.into_iter() {
                                    row.add_button(create_answer_button(answer, i));
                                }
                                row
                            })
                        })
                })
        })
        .await
        .expect("Couldn't send message");
}

fn create_answer_button(label: &str, answer_index: usize) -> CreateButton {
    let mut button = CreateButton::default();
    button
        .custom_id(format!("answer{answer_index}"))
        .label(label);
    button
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let config = serde_json::from_str::<BotConfig>(include_str!("../config.json"))
        .expect("Invalid config.json");

    let intents = GatewayIntents::empty();
    let mut client = Client::builder(token, intents)
        .event_handler(FanClubBot {
            fan_questions: config.fan_questions,
            fan_applications: DashMap::new(),
        })
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
