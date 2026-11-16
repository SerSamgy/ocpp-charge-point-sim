import asyncio
import logging
import os
from collections import defaultdict
from datetime import UTC, datetime

import websockets
from ocpp.exceptions import InternalError
from ocpp.routing import on
from ocpp.v16 import ChargePoint as cp
from ocpp.v16 import call_result
from ocpp.v16.datatypes import IdTagInfo
from ocpp.v16.enums import Action, AuthorizationStatus, RegistrationStatus

SERVER_HOST = "0.0.0.0"
SERVER_PORT = 9000
OCPP_PROTOCOL = "ocpp1.6"

# Boot responses are configurable for testing the client's retry logic:
#   BOOT_STATUSES=Pending,Rejected,Accepted  one status per BootNotification of a
#                                            charge point; the last one repeats
#   BOOT_INTERVAL=3                          the "interval" field, in seconds
BOOT_STATUSES = [
    RegistrationStatus(status.strip())
    for status in os.environ.get("BOOT_STATUSES", "Accepted").split(",")
]
BOOT_INTERVAL = int(os.environ.get("BOOT_INTERVAL", "10"))


def id_tags(name: str) -> set[str]:
    """A comma-separated list of idTags from the environment, case-folded:
    OCPP 1.6 compares idTags without regard to case (CiString20Type)."""
    return {
        tag.strip().casefold()
        for tag in os.environ.get(name, "").split(",")
        if tag.strip()
    }


# Authorization is configurable for testing the client's transactions:
#   BLOCKED_ID_TAGS=BAD1,BAD2       Blocked in Authorize and StartTransaction
#   START_REJECTED_ID_TAGS=LATE1    Accepted in Authorize, Blocked in
#                                   StartTransaction (the client then stops
#                                   the transaction with reason DeAuthorized)
# Every other idTag is Accepted.
BLOCKED_ID_TAGS = id_tags("BLOCKED_ID_TAGS")
START_REJECTED_ID_TAGS = id_tags("START_REJECTED_ID_TAGS")


def fail_counts(name: str) -> dict[str, int]:
    """`Action=N,Action=N` from the environment."""
    counts = {}
    for item in os.environ.get(name, "").split(","):
        if item.strip():
            action, _, count = item.partition("=")
            counts[action.strip()] = int(count)
    return counts


# Failures on demand, for testing the client's retries (section 3.7.1):
#   FAIL_FIRST=StopTransaction=2,MeterValues=1   the first 2 StopTransaction
#                                                and the first MeterValues of
#                                                this run get a CALLERROR
#                                                (InternalError)
FAIL_FIRST = fail_counts("FAIL_FIRST")


def fail_first(action: str) -> None:
    """Raises, so the `ocpp` library answers with CALLERROR, while the
    failures FAIL_FIRST asks for `action` are not used up yet."""
    left = FAIL_FIRST.get(action, 0)
    if left > 0:
        FAIL_FIRST[action] = left - 1
        logger.warning(f"{action}: failing on demand, {left - 1} failures left")
        raise InternalError(description="failing on demand (FAIL_FIRST)")


# Transaction IDs are unique per CSMS run, starting at 1.
next_transaction_id = 1
# Open transactions: ID -> (charge point ID, idTag, meterStart).
open_transactions: dict[int, tuple[str, str, int]] = {}
# The last energy register reported per open transaction, in Wh.
last_register: dict[int, float] = {}

# Section 4.7: a sampled value without these fields is a register reading
# of active import energy in Wh.
ENERGY_REGISTER = "Energy.Active.Import.Register"


def energy_register_wh(sampled_value: dict) -> float | None:
    """The value in Wh if it is the active import energy register, else None.
    The value is a string (it may be signed data), so a non-number is None."""
    if sampled_value.get("measurand", ENERGY_REGISTER) != ENERGY_REGISTER:
        return None
    unit = sampled_value.get("unit", "Wh")
    try:
        value = float(sampled_value["value"])
    except KeyError, ValueError:
        return None
    if unit == "Wh":
        return value
    if unit == "kWh":
        return value * 1000
    return None


# BootNotifications received per charge point ID. Kept across connections,
# because a rejected charge point reconnects before booting again.
boot_counts: defaultdict[str, int] = defaultdict(int)

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
        count = boot_counts[self.id]
        boot_counts[self.id] += 1
        status = BOOT_STATUSES[min(count, len(BOOT_STATUSES) - 1)]
        logger.info(f"Answering {status} with interval {BOOT_INTERVAL}")
        return call_result.BootNotification(
            current_time=now_iso(),
            interval=BOOT_INTERVAL,
            status=status,
        )

    @on(Action.heartbeat)
    def on_heartbeat(self):
        return call_result.Heartbeat(current_time=now_iso())

    @on(Action.status_notification)
    def on_status_notification(
        self, connector_id, error_code, status, timestamp=None, **kwargs
    ):
        logger.info(
            f"StatusNotification from {self.id}: connector={connector_id} "
            f"status={status} error_code={error_code} timestamp={timestamp}"
        )
        return call_result.StatusNotification()

    @on(Action.authorize)
    def on_authorize(self, id_tag):
        status = (
            AuthorizationStatus.blocked
            if id_tag.casefold() in BLOCKED_ID_TAGS
            else AuthorizationStatus.accepted
        )
        logger.info(f"Authorize from {self.id}: id_tag={id_tag} -> {status}")
        return call_result.Authorize(id_tag_info=IdTagInfo(status=status))

    @on(Action.start_transaction)
    def on_start_transaction(
        self, connector_id, id_tag, meter_start, timestamp, **kwargs
    ):
        global next_transaction_id
        transaction_id = next_transaction_id
        next_transaction_id += 1
        tag = id_tag.casefold()
        status = (
            AuthorizationStatus.blocked
            if tag in BLOCKED_ID_TAGS or tag in START_REJECTED_ID_TAGS
            else AuthorizationStatus.accepted
        )
        # Section 4.8: the CSMS answers with a transaction ID even when it
        # does not accept the idTag; the charge point then stops it.
        open_transactions[transaction_id] = (self.id, id_tag, meter_start)
        logger.info(
            f"StartTransaction from {self.id}: connector={connector_id} id_tag={id_tag} "
            f"meter_start={meter_start} timestamp={timestamp} "
            f"-> transaction_id={transaction_id} {status}"
        )
        return call_result.StartTransaction(
            transaction_id=transaction_id, id_tag_info=IdTagInfo(status=status)
        )

    @on(Action.meter_values)
    def on_meter_values(self, connector_id, meter_value, **kwargs):
        fail_first("MeterValues")
        transaction_id = kwargs.get("transaction_id")
        if transaction_id is not None and transaction_id not in open_transactions:
            logger.warning(
                f"MeterValues from {self.id} for unknown or closed transaction_id={transaction_id}"
            )
        for sample in meter_value:
            for sampled_value in sample["sampled_value"]:
                logger.info(
                    f"MeterValues from {self.id}: connector={connector_id} "
                    f"transaction_id={transaction_id} timestamp={sample['timestamp']} "
                    f"{sampled_value.get('measurand', ENERGY_REGISTER)}="
                    f"{sampled_value['value']} {sampled_value.get('unit', 'Wh')}"
                )
                register = energy_register_wh(sampled_value)
                if register is None or transaction_id not in open_transactions:
                    continue
                previous = last_register.get(transaction_id)
                if previous is not None and register < previous:
                    logger.warning(
                        f"energy register of transaction_id={transaction_id} went "
                        f"down from {previous} to {register} Wh"
                    )
                last_register[transaction_id] = register
        # Always an ordinary answer: an error would only make the charge
        # point send the same values again (section 4.7).
        return call_result.MeterValues()

    @on(Action.stop_transaction)
    def on_stop_transaction(self, meter_stop, timestamp, transaction_id, **kwargs):
        fail_first("StopTransaction")
        reason = kwargs.get("reason", "Local (absent)")
        id_tag = kwargs.get("id_tag")
        started = open_transactions.pop(transaction_id, None)
        last_sampled = last_register.pop(transaction_id, None)
        if last_sampled is not None and meter_stop < last_sampled:
            logger.warning(
                f"meter_stop={meter_stop} of transaction_id={transaction_id} is below "
                f"the last MeterValues register {last_sampled} Wh"
            )
        if started is None:
            # A repeated or unknown StopTransaction still gets an ordinary
            # answer: an error would make the charge point send it again
            # and again (section 3.7.1).
            logger.warning(
                f"StopTransaction from {self.id} for unknown or closed "
                f"transaction_id={transaction_id}: meter_stop={meter_stop} reason={reason}"
            )
        else:
            _, _, meter_start = started
            logger.info(
                f"StopTransaction from {self.id}: transaction_id={transaction_id} "
                f"id_tag={id_tag} meter_stop={meter_stop} "
                f"energy={meter_stop - meter_start} Wh reason={reason} timestamp={timestamp}"
            )
        return call_result.StopTransaction()


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
    except websockets.exceptions.ConnectionClosed as exc:
        # The Close frame the charge point sent, if any (RFC 6455 7.4.1).
        close = exc.rcvd
        reason = f"code {close.code} {close.reason!r}" if close else "no Close frame"
        logger.info(f"Charge point {charge_point_id} disconnected: {reason}")


async def main():
    server = await websockets.serve(
        on_connect, SERVER_HOST, SERVER_PORT, subprotocols=[OCPP_PROTOCOL]
    )
    logger.info(f"CSMS listening on ws://{SERVER_HOST}:{SERVER_PORT}/<charge_point_id>")
    await server.wait_closed()


if __name__ == "__main__":
    asyncio.run(main())
