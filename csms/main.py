import asyncio
import logging
from datetime import UTC, datetime

import websockets
from ocpp.routing import on
from ocpp.v16 import ChargePoint as cp
from ocpp.v16 import call_result
from ocpp.v16.enums import Action, RegistrationStatus

SERVER_HOST = "0.0.0.0"
SERVER_PORT = 9000
OCPP_PROTOCOL = "ocpp1.6"
HEARTBEAT_INTERVAL = 10

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)
logger.setLevel(logging.DEBUG)


def now_iso() -> str:
    return datetime.now(UTC).isoformat()


class ChargePoint(cp):
    """Server representation of a Charging Point"""

    @on(Action.boot_notification)
    def on_boot_notification(self, charge_point_vendor, charge_point_model, **kwargs):
        logger.info(
            f"BootNotification from {self.id}: vendor={charge_point_vendor} model={charge_point_model}"
        )
        return call_result.BootNotification(
            current_time=now_iso(),
            interval=HEARTBEAT_INTERVAL,
            status=RegistrationStatus.accepted,
        )

    @on(Action.heartbeat)
    def on_heartbeat(self):
        return call_result.Heartbeat(current_time=now_iso())


async def on_connect(websocket):
    """Called for every new websocket connection."""

    if not websocket.subprotocol:
        logger.warning("No matching OCPP subprotocol, closing connection")
        return await websocket.close()

    charge_point_id = websocket.request.path.split("/")[-1]
    if not charge_point_id:
        logger.warning(f"No charge point ID in path {websocket.request.path}")
        return await websocket.close()

    logger.info(f"Charge point {charge_point_id} connected ({websocket.subprotocol})")
    try:
        await ChargePoint(charge_point_id, websocket).start()
    except websockets.exceptions.ConnectionClosed:
        logger.info(f"Charge point {charge_point_id} disconnected")


async def main():
    server = await websockets.serve(
        on_connect, SERVER_HOST, SERVER_PORT, subprotocols=[OCPP_PROTOCOL]
    )
    logger.info(f"CSMS listening on ws://{SERVER_HOST}:{SERVER_PORT}/<charge_point_id>")
    await server.wait_closed()


if __name__ == "__main__":
    asyncio.run(main())
