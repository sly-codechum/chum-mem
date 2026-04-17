local json = require("cjson")
local log = require("log")

-- WHY: We use a module table pattern instead of globals so each
-- test can get its own isolated config instance.
local Config = {}
Config.__index = Config

--- Create a new Config with defaults.
-- @param overrides table Optional key-value overrides
-- @return Config
function Config.new(overrides)
    local self = setmetatable({}, Config)
    self.data = {
        port = 8080,
        debug = false,
        max_conns = 256,
    }
    if overrides then
        for k, v in pairs(overrides) do
            self.data[k] = v
        end
    end
    log.info("Config created with " .. Config.count(self) .. " keys")
    return self
end

-- NOTE: Returns 0 for an empty config, not nil.
function Config:count()
    local n = 0
    for _ in pairs(self.data) do n = n + 1 end
    return n
end

function Config:to_json()
    return json.encode(self.data)
end

local cfg = Config.new({ debug = true })
print(cfg:to_json())
print("Keys: " .. cfg:count())
