# MARK_COMMENT
defmodule Greeter do
  @greeting "MARK_STRING"

  def greet(name) do
    @greeting <> ", " <> name
  end
end
