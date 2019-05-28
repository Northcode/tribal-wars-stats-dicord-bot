FROM debian:latest

RUN apt-get update && apt-get --assume-yes install libssl1.1 openssl ca-certificates

WORKDIR /app
ADD target/release/tw-discord-bot /app

CMD [ "/app/tw-discord-bot" ]
