#![allow(deprecated)] // serenity uses deprecated type aliases in model::prelude

use async_trait::async_trait;
use dashmap::DashMap;
use dotenv::dotenv;
use rand::prelude::SliceRandom;
use serde::Deserialize;
use serenity::builder::CreateButton;
use serenity::model::prelude::application_command::ApplicationCommandInteraction;
use serenity::model::prelude::interaction::message_component::MessageComponentInteraction;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::env;
use std::time::{Duration, Instant};

const COMMAND_SETUP_FAN_APPLICATION_CHANNEL: &str = "setupfanapplicationchannel";
const BUTTON_FAN_APPLICATION: &str = "buttonsetupapplication";

struct FanClubBot {
    config: BotConfig,
    fan_applications: DashMap<UserId, FanApplication>,
}

#[derive(Deserialize)]
struct BotConfig {
    fan_questions: Vec<Question>,
    fan_application_questions: usize,
    fan_application_minutes: u64,
    fan_role: RoleId,
}

#[derive(Clone, Debug, Deserialize)]
struct Question {
    question: String,
    answers: Vec<String>,
    correct_answer: usize,
}

struct FanApplication {
    questions: Vec<Question>,
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
                        self.setup_fan_application_channel(&ctx, &command_interaction)
                            .await;
                    }
                    unknown => println!("Unknown command: {unknown}"),
                }
            }
            Interaction::MessageComponent(component_interaction) => {
                match &component_interaction.data.custom_id[..] {
                    BUTTON_FAN_APPLICATION => {
                        self.start_fan_application_process(&ctx, &component_interaction)
                            .await;
                    }
                    "answer0" | "answer1" | "answer2" | "answer3" => {
                        let i = component_interaction
                            .data
                            .custom_id
                            .chars()
                            .nth("answer".len())
                            .unwrap()
                            .to_string()
                            .parse::<usize>()
                            .unwrap();
                        self.process_answer(&ctx, &component_interaction, i).await;
                    }
                    unknown => println!("Unknown component interaction: {unknown}"),
                }
            }
            _ => (),
        }
    }
}

impl FanClubBot {
    async fn setup_fan_application_channel(
        &self,
        ctx: &Context,
        command_interaction: &ApplicationCommandInteraction,
    ) {
        command_interaction
            .channel_id
            .send_message(&ctx.http, |message| {
                message
                    .content(format!("\
                Если хочешь стать более уважаемым членом сообщества, открыть каналы для истинных фанатов и получить роль <@&{fan_role}>, нажми эту кнопку.\n\
                После нажатия кнопки тебе зададут {questions} простых вопросов по A4, на которые тебе нужно будет правильно ответить за {minutes} минут.\
                ", fan_role = self.config.fan_role.0, questions = self.config.fan_application_questions, minutes = self.config.fan_application_minutes))
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

    async fn start_fan_application_process(
        &self,
        ctx: &Context,
        component_interaction: &MessageComponentInteraction,
    ) {
        assert!(self.config.fan_questions.len() >= self.config.fan_application_questions);
        let mut questions_index = (0..self.config.fan_questions.len()).collect::<Vec<_>>();
        questions_index.shuffle(&mut rand::thread_rng());
        let questions = questions_index
            .into_iter()
            .take(self.config.fan_application_questions)
            .map(|i| &self.config.fan_questions[i])
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(questions.len(), self.config.fan_application_questions);
        let application_state = FanApplication {
            questions,
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
        self.ask_question(
            ctx,
            component_interaction,
            &application_state.questions[application_state.current_question],
        )
        .await;
    }

    async fn ask_question(
        &self,
        ctx: &Context,
        component_interaction: &MessageComponentInteraction,
        question: &Question,
    ) {
        component_interaction
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
                                        row.add_button(Self::create_answer_button(answer, i));
                                    }
                                    row
                                })
                            })
                    })
            })
            .await
            .expect("Couldn't send message");
    }

    async fn process_answer(
        &self,
        ctx: &Context,
        component_interaction: &MessageComponentInteraction,
        answer_index: usize,
    ) {
        let mut member = component_interaction
            .member
            .clone()
            .expect("Couldn't get interaction author");
        let mut application_state = self.fan_applications.get_mut(&member.user.id).unwrap();
        assert_eq!(
            application_state.questions.len(),
            self.config.fan_application_questions
        );

        let current_question = &application_state.questions[application_state.current_question];
        assert!(answer_index < current_question.answers.len());

        if current_question.correct_answer == answer_index {
            if application_state.current_question + 1 == application_state.questions.len() {
                if application_state.start_time
                    + Duration::from_secs(60 * self.config.fan_application_minutes)
                    > Instant::now()
                {
                    if !member.roles.contains(&self.config.fan_role) {
                        member
                            .add_role(&ctx.http, &self.config.fan_role)
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
                self.ask_question(
                    ctx,
                    component_interaction,
                    &application_state.questions[application_state.current_question],
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

    fn create_answer_button(label: &str, answer_index: usize) -> CreateButton {
        let mut button = CreateButton::default();
        button
            .custom_id(format!("answer{answer_index}"))
            .label(label);
        button
    }
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
            config,
            fan_applications: DashMap::new(),
        })
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
