defmodule Worker do
  @moduledoc """
  GenServer-based worker that processes jobs from a queue.

  WHY: We use GenServer instead of Task.async because we need
  supervision and automatic restart on crash.
  """

  use GenServer
  require Logger

  defstruct [:name, :queue, processed: 0]

  # --- Client API ---

  def start_link(opts) do
    name = Keyword.fetch!(opts, :name)
    GenServer.start_link(__MODULE__, opts, name: via(name))
  end

  def enqueue(worker_name, job) do
    GenServer.cast(via(worker_name), {:enqueue, job})
  end

  def stats(worker_name) do
    GenServer.call(via(worker_name), :stats)
  end

  # --- Server Callbacks ---

  @impl true
  def init(opts) do
    Logger.info("Worker #{opts[:name]} starting")
    {:ok, %__MODULE__{name: opts[:name], queue: :queue.new()}}
  end

  @impl true
  def handle_cast({:enqueue, job}, state) do
    # NOTE: We process inline for now; move to Task if jobs become slow.
    Logger.info("Processing job: #{inspect(job)}")
    {:noreply, %{state | processed: state.processed + 1}}
  end

  @impl true
  def handle_call(:stats, _from, state) do
    {:reply, %{name: state.name, processed: state.processed}, state}
  end

  defp via(name), do: {:via, Registry, {Worker.Registry, name}}
end
