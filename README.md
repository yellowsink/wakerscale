# Wakerscale

Let your tailscale devices send WOL packets to the LAN.

Create `wakerscale.json` like the following:

```json
{
	"hostname": "milkzel-wakerscale",
	"iface": "eth0",
	"port": 80,
	"passwords": [
		"abcd1234"
	]
}
```

You can also optionally specify `"controlserver": ""`.
An auth key will be read from the `TS_AUTHKEY` env var, or a tailscale register link will be printed to the logs.

Then start the server, and `curl -X POST milkzel-wakerscale/wake/88:00:33:77:7F:43 -H 'Authorization: abcd1234'`
to send a WOL packet to `88:00:33:77:7F:43`.