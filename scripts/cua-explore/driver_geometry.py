"""Lend the harness's own accessibility walk to a driver snapshot."""

from __future__ import annotations

from typing import Any, Mapping


class MeasuredGeometry:
    """Mixed into CuaExecutor: positions, and the headers, the driver cannot give.

    Reads `geometry_provider`, `window_origin`, `hover_geometry`, `column_headers`,
    `generation` and the `geometry_*` records of the executor it is mixed into.
    """

    def with_measured_geometry(
        self, raw: Mapping[str, Any], *, state_id: str
    ) -> Mapping[str, Any]:
        """Replace the driver's placeholder positions with measured ones."""
        origin = self.window_origin or self.hover_geometry
        if self.geometry_provider is None or origin is None:
            return raw
        from atspi_geometry import (
            GeometryError,
            column_header_elements,
            resolve_driver_geometry,
        )

        structured = raw.get("structuredContent")
        container = structured if isinstance(structured, dict) else raw
        elements = container.get("elements")
        if not isinstance(elements, list):
            failure = "snapshot carries no element list"
            self.geometry_failures.append(failure)
            self._record_geometry(state_id, trusted=False, failure=failure)
            return self._untrusted(raw, failure)
        try:
            nodes = self.geometry_provider()
            resolution = resolve_driver_geometry(elements, nodes, origin)
        except GeometryError as error:
            failure = str(error)
            self.geometry_failures.append(failure)
            self._record_geometry(state_id, trusted=False, failure=failure)
            return self._untrusted(raw, failure)
        self.geometry_calibration = resolution.calibration
        self.geometry_resolution = resolution.as_record()
        frames = resolution.frames
        if not resolution.trusted:
            failure = (
                "no element could be matched to a measured position "
                f"({resolution.driver_elements} driver elements, "
                f"{resolution.walk_nodes} walk nodes)"
            )
            self.geometry_failures.append(failure)
            self._record_geometry(
                state_id,
                trusted=False,
                failure=failure,
                resolution=self.geometry_resolution,
                calibration=self.geometry_calibration,
            )
            return self._untrusted(raw, "no element resolved")
        self._record_geometry(
            state_id,
            trusted=True,
            resolution=self.geometry_resolution,
            calibration=self.geometry_calibration,
        )
        rebuilt = []
        for index, element in enumerate(elements):
            if not isinstance(element, dict):
                rebuilt.append(element)
                continue
            rect = frames.get(int(element.get("element_index", index)))
            if rect is None:
                rebuilt.append({**element, "geometry_trusted": False})
                continue
            rebuilt.append(
                {
                    **element,
                    "geometry_trusted": True,
                    "actions": list(resolution.actions.get(
                        int(element.get("element_index", index)), ()
                    )),
                    "frame": {
                        "x": rect[0],
                        "y": rect[1],
                        "w": rect[2],
                        "h": rect[3],
                    },
                }
            )
        if self.column_headers:
            rebuilt.extend(column_header_elements(nodes, origin))
        if container is raw:
            return {**raw, "elements": rebuilt, "geometry_trusted": True}
        return {
            **raw,
            "structuredContent": {**structured, "elements": rebuilt},
            "geometry_trusted": True,
        }

    def _record_geometry(
        self,
        state_id: str,
        *,
        trusted: bool,
        failure: str | None = None,
        resolution: Mapping[str, Any] | None = None,
        calibration: Mapping[str, Any] | None = None,
    ) -> None:
        record: dict[str, Any] = {
            "generation": self.generation,
            "state_id": state_id,
            "trusted": trusted,
        }
        if failure is not None:
            record["failure"] = failure
        if resolution is not None:
            record["resolution"] = dict(resolution)
        if calibration is not None:
            record["calibration"] = dict(calibration)
        self.geometry_measurements.append(record)

    @staticmethod
    def _untrusted(raw: Mapping[str, Any], note: str) -> Mapping[str, Any]:
        return {**raw, "geometry_trusted": False, "geometry_note": note}
