require "json"
require "logger"

# WHY: We inherit from BaseHandler so middleware (auth, rate-limit)
# is applied automatically via the handler chain.
class RequestHandler
  attr_reader :logger

  def initialize(config = {})
    @logger = Logger.new($stdout)
    @config = config
    @logger.info("Handler initialized with #{config.keys.length} options")
  end

  # Process an incoming request hash and return a response hash.
  # NOTE: Always returns a hash — never raises across the boundary.
  def handle(request)
    validate!(request)
    body = JSON.parse(request[:body] || "{}")
    result = transform(body)
    { status: 200, body: JSON.generate(result) }
  rescue StandardError => e
    @logger.error("Request failed: #{e.message}")
    { status: 500, body: JSON.generate({ error: e.message }) }
  end

  private

  def validate!(request)
    raise ArgumentError, "missing :method" unless request.key?(:method)
  end

  def transform(data)
    data.transform_keys(&:to_sym).merge(processed_at: Time.now.iso8601)
  end
end

handler = RequestHandler.new(verbose: true)
response = handler.handle(method: "POST", body: '{"name":"test"}')
puts response[:body]
