import asyncio
import json
import os
from typing import Any, Callable, Dict

import pika

RABBITMQ_HOST = os.getenv("RABBITMQ_HOST", "localhost")
RABBITMQ_USER = os.getenv("RABBITMQ_USER", "guest")
RABBITMQ_PASS = os.getenv("RABBITMQ_PASS", "guest")


class RabbitMQProducer:
    def __init__(self):
        self.connection = None
        self.channel = None
        self.connected = False

    async def connect(self):
        if self.connected:
            return
        try:
            self.connection = pika.BlockingConnection(
                pika.ConnectionParameters(
                    host=RABBITMQ_HOST, credentials=pika.PlainCredentials(RABBITMQ_USER, RABBITMQ_PASS)
                )
            )
            self.channel = self.connection.channel()
            self.connected = True
            print(f"Connected to RabbitMQ at {RABBITMQ_HOST}")
        except pika.exceptions.AMQPConnectionError as e:
            print(f"Failed to connect to RabbitMQ: {e}")
            self.connected = False
            # Implement retry logic if needed

    def close(self):
        if self.connected and self.connection:
            self.connection.close()
            self.connected = False
            print("Disconnected from RabbitMQ.")

    def publish(self, queue_name: str, message: Dict[str, Any]):
        if not self.connected:
            # Attempt to reconnect or raise error
            print("RabbitMQ producer not connected. Attempting to reconnect...")
            asyncio.run(self.connect())  # Call async connect in a synchronous context if needed
            if not self.connected:
                raise Exception("RabbitMQ producer not connected and failed to reconnect.")

        self.channel.queue_declare(queue=queue_name, durable=True)
        self.channel.basic_publish(
            exchange="",
            routing_key=queue_name,
            body=json.dumps(message),
            properties=pika.BasicProperties(delivery_mode=pika.DeliveryMode.Persistent),  # Make message persistent
        )
        # print(f"Published message to queue '{queue_name}'")


class RabbitMQConsumer:
    def __init__(self, queue_name: str, callback: Callable[[Dict[str, Any]], None]):
        self.queue_name = queue_name
        self.callback = callback
        self.connection = None
        self.channel = None

    def connect(self):
        self.connection = pika.BlockingConnection(
            pika.ConnectionParameters(
                host=RABBITMQ_HOST, credentials=pika.PlainCredentials(RABBITMQ_USER, RABBITMQ_PASS)
            )
        )
        self.channel = self.connection.channel()
        self.channel.queue_declare(queue=self.queue_name, durable=True)
        self.channel.basic_consume(queue=self.queue_name, on_message_callback=self._on_message)
        print(f"Waiting for messages in queue '{self.queue_name}'. To exit press CTRL+C")

    def _on_message(self, ch, method, properties, body):
        try:
            message_data = json.loads(body)
            asyncio.run(self.callback(message_data))  # Execute callback
            ch.basic_ack(delivery_tag=method.delivery_tag)
        except Exception as e:
            print(f"Error processing message: {e}")
            ch.basic_nack(delivery_tag=method.delivery_tag, requeue=True)  # Requeue message on error

    def start_consuming(self):
        self.channel.start_consuming()

    def stop_consuming(self):
        if self.channel:
            self.channel.stop_consuming()
        if self.connection:
            self.connection.close()
