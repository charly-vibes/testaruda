@testitem "greet returns correct greeting" begin
    using Test
    include("../src/MyFixture.jl")
    using .MyFixture
    @test MyFixture.greet("World") == "Hello, World!"
end

@testitem "add returns sum" begin
    using Test
    include("../src/MyFixture.jl")
    using .MyFixture
    @test MyFixture.add(2, 3) == 5
    @test MyFixture.add(-1, 1) == 0
end

@testitem "is_positive returns correct bool" begin
    using Test
    include("../src/MyFixture.jl")
    using .MyFixture
    @test MyFixture.is_positive(5) == true
    @test MyFixture.is_positive(-1) == false
    @test MyFixture.is_positive(0) == false
end